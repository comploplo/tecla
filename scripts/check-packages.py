#!/usr/bin/env python3
"""Build release archives using only sources extracted outside the checkout.

Cargo's --no-verify is used only to create the archives before Tecla's dependencies
are on crates.io. Verification happens afterwards against those exact archives;
the temporary Cargo patches never point back at workspace source directories.
"""

import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tarfile
import tempfile


ROOT = Path(__file__).resolve().parents[1]
PACKAGES = (
    "tecla-config", "tecla", "tecla-macros", "tecla-rp2040", "tecla-nrf52840",
    "tecla-stm32f4", "tecla-stm32f3", "tecla-stm32f0",
)
TARGETS = {
    "tecla-rp2040": "thumbv6m-none-eabi",
    "tecla-nrf52840": "thumbv7em-none-eabihf",
    "tecla-stm32f4": "thumbv7em-none-eabihf",
    "tecla-stm32f3": "thumbv7em-none-eabihf",
    "tecla-stm32f0": "thumbv6m-none-eabi",
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true", help="use cached dependencies only")
    args = parser.parse_args()

    env = os.environ.copy()
    # Keep the repository toolchain when cargo runs from the temporary directory.
    channel = re.search(r'^channel\s*=\s*"([^"]+)"',
                        (ROOT / "rust-toolchain.toml").read_text(), re.MULTILINE)
    if channel is None:
        raise RuntimeError("rust-toolchain.toml is missing its pinned channel")
    env["RUSTUP_TOOLCHAIN"] = channel.group(1)
    cache = ROOT / "target" / "package-check"
    env["CARGO_TARGET_DIR"] = str(cache)

    def cargo(cwd, *command, config=None, capture=False, extra_env=None):
        argv = ["cargo"]
        if args.offline:
            argv.append("--offline")
        if config is not None:
            argv += ["--config", str(config)]
        argv += list(command)
        print(f"[{cwd.name}] {shlex.join(argv)}", flush=True)
        result = subprocess.run(argv, cwd=cwd, env={**env, **(extra_env or {})},
                                text=True, stdout=subprocess.PIPE if capture else None,
                                check=True)
        return json.loads(result.stdout) if capture else None

    source = cargo(ROOT, "metadata", "--no-deps", "--format-version", "1", capture=True)
    packages = {p["name"]: p for p in source["packages"]
                if p["id"] in source["workspace_members"] and p["publish"] != []}
    if set(packages) != set(PACKAGES):
        raise RuntimeError("update the package validation matrix for publishable crates: "
                           + ", ".join(sorted(packages)))

    with tempfile.TemporaryDirectory(prefix="tecla-package-check-") as directory:
        stage = Path(directory).resolve()
        if ROOT == stage or ROOT in stage.parents:
            raise RuntimeError("TMPDIR must be outside the repository")
        config = stage / "patches.toml"
        extracted = {}

        # Cargo stages workspace dependencies itself; keep temporary patches out
        # of the checkout's lockfile. This creates archives, not verification.
        selection = [arg for name in PACKAGES for arg in ("--package", name)]
        cargo(ROOT, "package", *selection, "--allow-dirty", "--no-verify", "--locked")
        for name in PACKAGES:
            package = packages[name]
            archive = cache / "package" / f"{name}-{package['version']}.crate"
            with tarfile.open(archive) as contents:
                for member in contents.getmembers():
                    destination = (stage / member.name).resolve()
                    if stage not in destination.parents or not (member.isfile() or member.isdir()):
                        raise RuntimeError(f"unexpected archive entry: {member.name}")
                contents.extractall(stage)
            extracted[name] = stage / f"{name}-{package['version']}"
        config.write_text("[patch.crates-io]\n" + "".join(
            f"{name} = {{ path = {json.dumps(str(extracted[name]))} }}\n"
            for name in PACKAGES[:3]))

        metadata = {}
        for name, path in extracted.items():
            for license_name in ("LICENSE-MIT", "LICENSE-APACHE"):
                if not (path / license_name).is_file():
                    raise RuntimeError(f"{name}: missing packaged {license_name}")
            if name == "tecla-rp2040" and not (path / "THIRD-PARTY-NOTICES").is_file():
                raise RuntimeError("tecla-rp2040: missing packaged dictionary license notice")
            manifest = cargo(path, "metadata", "--no-deps", "--format-version", "1",
                             config=config, capture=True)
            package = next(p for p in manifest["packages"] if p["name"] == name)
            metadata[name] = package
            for dependency in package["dependencies"]:
                if dependency.get("path") is not None:
                    raise RuntimeError(f"{name}: packaged path dependency {dependency['name']}")
                if dependency["name"] in PACKAGES and dependency["req"] == "*":
                    raise RuntimeError(f"{name}: unversioned dependency {dependency['name']}")
            for target in package["targets"]:
                if path not in Path(target["src_path"]).parents:
                    raise RuntimeError(f"{name}: target escapes archive: {target['src_path']}")

        def check(name, *command, **kwargs):
            return cargo(extracted[name], *command, config=config, **kwargs)

        for name in PACKAGES[:3]:
            check(name, "build")
        check("tecla-config", "build", "--features", "dictionary")
        for name in PACKAGES[:3]:
            features = ["--features", "testing"] if name == "tecla" else []
            check(name, "test", "--doc", *features)
        check("tecla", "check", "--lib", "--target", "thumbv6m-none-eabi",
              "--features", "usb,usb-midi,via,storage,embassy")

        for name, target in TARGETS.items():
            command = ["build", "--release", "--lib", "--examples", "--target", target]
            features = {"tecla-rp2040": "hil,steno-dictionary,usb-midi",
                        "tecla-nrf52840": "hil", "tecla-stm32f3": "usb-midi"}.get(name)
            check(name, *command, *(["--features", features] if features else []))
        check("tecla-rp2040", "build", "--release", "--example", "core1_autocorrect",
              "--target", TARGETS["tecla-rp2040"], "--features", "core1-autocorrect")
        check("tecla-nrf52840", "build", "--release", "--example", "planck_ble",
              "--target", TARGETS["tecla-nrf52840"], "--no-default-features", "--features", "ble")
        check("tecla-stm32f3", "build", "--release", "--lib", "--examples",
              "--target", TARGETS["tecla-stm32f3"])

        for name in PACKAGES:
            docs = (metadata[name]["metadata"] or {}).get("docs", {}).get("rs", {})
            # [package.metadata.docs.rs] becomes nested docs/rs metadata.
            target = docs.get("default-target")
            if name in TARGETS and (target != TARGETS[name] or docs.get("targets") != [target]):
                raise RuntimeError(f"{name}: docs.rs must select its embedded target")
            command = ["doc", "--no-deps"]
            if target:
                command += ["--target", target]
            if docs.get("all-features"):
                command += ["--all-features"]
            elif docs.get("features"):
                command += ["--features", ",".join(docs["features"])]
            if docs.get("no-default-features"):
                command += ["--no-default-features"]
            rustdoc = ["-D", "warnings"] + docs.get("rustdoc-args", [])
            check(name, *command, extra_env={"RUSTDOCFLAGS": shlex.join(rustdoc)})

    print(f"Verified {len(PACKAGES)} extracted release archives; no crates were published.")


if __name__ == "__main__":
    main()
