#!/usr/bin/env python3
"""Capture Voyager Harmony's USB serial diagnostics without sending any bytes.

Run with --wait before connecting telemetry firmware. Automatic discovery only
uses /dev/cu.usbmodem* names containing TECLA or HARMONY; use --port otherwise.
The JSONL preserves complete firmware frames, including attempted MIDI and LEDs.
"""

import argparse
from datetime import datetime, timezone
import errno
import fcntl
import glob
import json
import math
import os
from pathlib import Path
import select
import struct
import sys
import termios
import time


MAX_LINE = 8192
PITCHES = ("C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B")
SHAPES = {
    "Major": "", "Minor": "m", "Diminished": "dim", "Augmented": "+",
    "Major6": "6", "Minor6": "m6", "Add9": "add9", "MinorAdd9": "m(add9)",
    "Major7": "maj7", "Minor7": "m7", "Dominant7": "7",
    "HalfDiminished7": "m7b5", "Diminished7": "dim7", "MinorMajor7": "m(maj7)",
    "Major9": "maj9", "Minor9": "m9", "Dominant9": "9", "DominantFlat13": "7(b13)",
    "Sus2": "sus2", "Sus4": "sus4", "DominantSus4": "7sus4",
    "Dominant13": "13", "Major13": "maj13", "DominantFlat9": "7(b9)",
    "DominantSharp9": "7(#9)",
}
CAUSES = (
    "LH+", "LH-", "RHS+", "RHS-", "delay-release", "restrike", "answer",
    "remember", "recovery", "replan", "stop",
)
DIRECTIONS = ("Continue", "Prepare", "Lean", "Arrive", "Deceive", "Explore", "Substitute")
INTENTS = {
    "colour": ("Natural", "SecondsFourths", "Sixth", "Seventh", "Extended"),
    "foundation": ("Natural", "Root", "Third", "Fifth", "Seventh"),
    "motion": ("Nearest", "Falling", "Rising", "Inward", "Outward"),
    "reharm": ("Faithful", "Relative", "CommonTone", "Borrowed", "AppliedDominant"),
}


def integer(value, high):
    return type(value) is int and 0 <= value <= high


def note(value):
    return value is None or integer(value, 127)


def note_array(value, size):
    return isinstance(value, list) and len(value) == size and all(note(n) for n in value)


def preferences(value, version):
    if not isinstance(value, dict) or type(value.get("delay")) is not bool:
        return False
    if version == 1:
        return (all(integer(value.get(k), 3) for k in ("colour", "structure", "movement", "reinterpretation"))
                and value.get("direction") in DIRECTIONS[:4])
    return (all(value.get(name) in choices for name, choices in INTENTS.items())
            and value.get("direction") in DIRECTIONS
            and type(value.get("pedal")) is bool)


def chord(value):
    return value is None or (isinstance(value, dict)
                             and integer(value.get("root"), 11)
                             and value.get("shape") in SHAPES
                             and integer(value.get("tones"), 4095))


def validate(frame):
    """Reject broken/out-of-range frames before rendering or recording them."""
    if not isinstance(frame, dict) or type(frame.get("v")) is not int or frame["v"] not in (1, 2):
        raise ValueError("expected telemetry schema v1 or v2")
    version = frame["v"]
    for name, high in (("seq", 0xFFFFFFFF), ("event_ms", 0xFFFFFFFFFFFFFFFF),
                       ("time_ms", 0xFFFFFFFFFFFFFFFF), ("cause", 65535),
                       ("physical", (1 << 52) - 1), ("accepted", (1 << 52) - 1),
                       ("suspended", 63)):
        if not integer(frame.get(name), high):
            raise ValueError(f"invalid {name}")
    for name, size in (("captured", 24), ("support", 6), ("resolved", 6), ("tendencies", 6)):
        if not note_array(frame.get(name), size):
            raise ValueError(f"invalid {name} notes")
    if version == 1 and ("answer" not in frame or not note(frame["answer"])):
        raise ValueError("invalid answer note")
    if version == 2 and not integer(frame.get("limited"), 15):
        raise ValueError("invalid limited intent mask")
    if not all(preferences(frame.get(name), version) for name in ("requested", "applied")):
        raise ValueError("invalid preferences")
    if not all(name in frame and chord(frame[name]) for name in ("reference", "chord", "destination")):
        raise ValueError("invalid chord")
    if "root_implied" in frame and type(frame["root_implied"]) is not bool:
        raise ValueError("invalid root_implied")
    transport = frame.get("transport")
    if not isinstance(transport, dict):
        raise ValueError("missing transport")
    if not all(type(transport.get(k)) is bool for k in ("ready", "midi_ready", "right_ok", "recovered", "tx_fault")):
        raise ValueError("invalid transport flags")
    for name, high in (("right_age_ms", 0xFFFFFFFFFFFFFFFF), ("session", 0xFFFFFFFF),
                       ("dropped", 0xFFFFFFFF), ("update_us", 0xFFFFFFFF)):
        if not integer(transport.get(name), high):
            raise ValueError(f"invalid transport.{name}")
    events = frame.get("events")
    if not isinstance(events, list) or len(events) > 128:
        raise ValueError("invalid MIDI event array")
    if not all(isinstance(event, list) and len(event) == 4
               and all(integer(byte, 255) for byte in event) for event in events):
        raise ValueError("invalid USB MIDI event")
    leds = frame.get("leds")
    if not isinstance(leds, str) or len(leds) != 312 or any(c not in "0123456789abcdefABCDEF" for c in leds):
        raise ValueError("expected 52 RGB LED values")
    return frame


class Decoder:
    """A partial-line buffer bounded even when a disconnected stream is damaged."""

    def __init__(self):
        self.buffer = bytearray()
        self.discarding = False

    def feed(self, data):
        frames = []
        rejected = 0
        last_error = None
        for part in data.splitlines(keepends=True):
            complete = part.endswith(b"\n")
            if self.discarding:
                if complete:
                    self.discarding = False
                continue
            if len(self.buffer) + len(part) > MAX_LINE:
                self.buffer.clear()
                self.discarding = not complete
                rejected += 1
                last_error = "oversized line discarded"
                continue
            self.buffer.extend(part)
            if not complete:
                continue
            raw = bytes(self.buffer).strip()
            self.buffer.clear()
            if not raw:
                continue
            try:
                frames.append(validate(json.loads(raw)))
            except (ValueError, UnicodeDecodeError, TypeError) as error:
                rejected += 1
                last_error = str(error)
        return frames, rejected, last_error


def note_name(value):
    return "-" if value is None else f"{PITCHES[value % 12]}{value // 12 - 1}"


def chord_name(value):
    return "-" if value is None else PITCHES[value["root"]] + SHAPES[value["shape"]]


def prefs_name(value, version):
    if version == 1:
        return (f"colour={value['colour']} structure={value['structure']} movement={value['movement']} "
                f"reinterpret={value['reinterpretation']} {value['direction']} delay={int(value['delay'])}")
    return (f"colour={value['colour']} foundation={value['foundation']} motion={value['motion']} "
            f"reharm={value['reharm']} {value['direction']} "
            f"delay={int(value['delay'])} pedal={int(value['pedal'])}")


def control_name(led, version=2):
    thumbs = {24: "octave-down", 25: "octave-up", 50: "restrike", 51: "arrive"}
    if led in thumbs:
        return thumbs[led]
    if not 26 <= led < 50:
        return f"key-{led}"
    row, col = divmod(led - 26, 6)
    if version == 1:
        if 1 <= col <= 4:
            return f"{('colour', 'structure', 'movement', 'reinterpret')[col - 1]}{3 - row}"
        return (("prepare", "delay", "continue", "remember") if col == 0
                else ("lean", "resolve", "substitute", "answer"))[row]
    return (
        ("prepare", "colour:Extended", "foundation:Seventh", "motion:Outward", "reharm:AppliedDominant", "lean"),
        ("delay", "colour:Seventh", "foundation:Root", "motion:Inward", "reharm:Borrowed", "resolve"),
        ("continue", "colour:Sixth", "foundation:Fifth", "motion:Rising", "reharm:CommonTone", "substitute"),
        ("pedal", "colour:SecondsFourths", "foundation:Third", "motion:Falling", "reharm:Relative", "deceive"),
    )[row][col]


class Sequence:
    def __init__(self):
        self.previous = None

    def observe(self, seq, at):
        warning = None
        if self.previous is not None:
            previous, time_ms = self.previous
            delta = (seq - previous) & 0xFFFFFFFF
            if at < time_ms or delta >= 0x80000000:
                warning = f"firmware sequence reset {previous}->{seq}"
            elif delta > 1:
                warning = f"sequence gap: {delta - 1} missing event(s), {previous}->{seq}"
        self.previous = (seq, at)
        return warning


class Renderer:
    def __init__(self, clashes=False):
        self.sequence = Sequence()
        self.previous = None
        self.clashes = clashes

    def show(self, frame, host_time):
        version = frame["v"]
        warning = self.sequence.observe(frame["seq"], frame["time_ms"])
        transport = frame["transport"]
        if warning:
            print(f"  ! {warning}", flush=True)
        old_dropped = self.previous["transport"]["dropped"] if self.previous else 0
        if transport["dropped"] != old_dropped:
            print(f"  ! serial snapshots dropped={transport['dropped']} (previous={old_dropped})", flush=True)
        if transport["tx_fault"]:
            print("  ! MIDI transmit fault: listed MIDI events were attempted; delivery is not confirmed", flush=True)
        cause_names = CAUSES if version == 1 else CAUSES[:6] + ("pedal", "deceive") + CAUSES[8:]
        causes = "+".join(name for bit, name in enumerate(cause_names) if frame["cause"] & (1 << bit)) or "status"
        literal = ",".join(note_name(n) for n in sorted(n for n in frame["captured"] if n is not None)) or "-"
        support = " ".join(f"v{i}:{note_name(n)}" for i, n in enumerate(frame["support"]) if n is not None) or "-"
        controls = ",".join(control_name(led, version) for led in range(24, 52) if frame["accepted"] & (1 << led)) or "-"
        answer = f" answer={note_name(frame['answer'])}" if version == 1 else ""
        print(f"{host_time} #{frame['seq']} t={frame['time_ms']}ms {causes} "
              f"LH=[{literal}] support=[{support}]{answer} "
              f"chord={chord_name(frame['chord'])} destination={chord_name(frame['destination'])}", flush=True)
        print(f"  controls=[{controls}] ready={int(transport['ready'])} MIDI={int(transport['midi_ready'])} "
              f"right={int(transport['right_ok'])}/age={transport['right_age_ms']}ms "
              f"session={transport['session']} recovered={int(transport['recovered'])} "
              f"update={transport['update_us']}us", flush=True)
        if (not self.previous or frame["requested"] != self.previous["requested"]
                or frame["applied"] != self.previous["applied"]):
            print(f"  requested: {prefs_name(frame['requested'], version)}\n"
                  f"  applied:   {prefs_name(frame['applied'], version)} reference={chord_name(frame['reference'])}", flush=True)
        if version == 2 and frame["limited"]:
            unmet = ", ".join(name for bit, name in enumerate(INTENTS) if frame["limited"] & (1 << bit))
            print(f"  limited by the held notes or voice range: {unmet}", flush=True)
        resolutions = []
        for voice, pitch in enumerate(frame["support"]):
            if frame["suspended"] & (1 << voice):
                resolutions.append(f"v{voice}:{note_name(pitch)}->{note_name(frame['resolved'][voice])} delayed")
            elif frame["tendencies"][voice] is not None:
                resolutions.append(f"v{voice}:{note_name(pitch)}->{note_name(frame['tendencies'][voice])}")
        if resolutions:
            print("  resolutions: " + "; ".join(resolutions), flush=True)
        events = []
        for _, status, pitch, velocity in frame["events"]:
            if status & 0xF0 in (0x80, 0x90):
                on = status & 0xF0 == 0x90 and velocity != 0
                events.append(("+" if on else "-") + note_name(pitch))
        if events:
            print("  attempted MIDI: " + " ".join(events), flush=True)
        if self.clashes:
            adjacent = {(min(a, b), max(a, b)) for a in frame["support"] if a is not None
                        for b in frame["captured"] if b is not None and abs(a - b) == 1}
            if adjacent:
                print("  adjacent semitones (may be intentional): "
                      + ", ".join(f"{note_name(a)}/{note_name(b)}" for a, b in sorted(adjacent)), flush=True)
        self.previous = frame


def discover(explicit):
    if explicit:
        return str(Path(explicit).expanduser()) if Path(explicit).expanduser().exists() else None
    candidates = [port for port in glob.glob("/dev/cu.usbmodem*")
                  if any(word in Path(port).name.upper() for word in ("TECLA", "HARMONY"))]
    if len(candidates) > 1:
        raise ValueError("multiple Tecla/Harmony ports; choose --port: " + ", ".join(sorted(candidates)))
    return candidates[0] if candidates else None


def close_serial(fd):
    # Explicitly end streaming even when the device/driver ignores HUPCL.
    try:
        fcntl.ioctl(fd, termios.TIOCMBIC, struct.pack("I", termios.TIOCM_DTR))
    except OSError:
        pass  # An unplugged serial device can no longer change modem lines.
    finally:
        try:
            os.close(fd)
        except OSError:
            pass


def open_serial(port):
    fd = os.open(port, os.O_RDONLY | os.O_NONBLOCK | os.O_NOCTTY)
    try:
        attrs = termios.tcgetattr(fd)
        attrs[0] = attrs[1] = attrs[3] = 0
        attrs[2] = termios.CLOCAL | termios.CREAD | termios.CS8 | termios.HUPCL
        attrs[4] = attrs[5] = termios.B115200
        attrs[6][termios.VMIN] = attrs[6][termios.VTIME] = 0
        termios.tcsetattr(fd, termios.TCSANOW, attrs)
        # DTR requests diagnostic streaming; no serial or MIDI bytes are sent.
        fcntl.ioctl(fd, termios.TIOCMBIS, struct.pack("I", termios.TIOCM_DTR))
        return fd
    except BaseException:
        close_serial(fd)
        raise


def self_test():
    from contextlib import redirect_stdout
    from io import StringIO
    from unittest.mock import patch

    prefs = {"colour": 0, "structure": 0, "movement": 0, "reinterpretation": 0,
             "direction": "Continue", "delay": False}
    frame = {"v": 1, "seq": 1, "event_ms": 3, "time_ms": 4, "cause": 1,
             "physical": 2, "accepted": 0,
             "transport": {"ready": True, "midi_ready": True, "right_ok": True,
                           "right_age_ms": 0, "session": 3, "recovered": False,
                           "tx_fault": False, "dropped": 0, "update_us": 120},
             "captured": [None, 60] + [None] * 22, "support": [48] + [None] * 5,
             "answer": None, "requested": prefs, "applied": prefs,
             "reference": None, "chord": {"root": 0, "shape": "Major", "tones": 145},
             "destination": None, "suspended": 0, "resolved": [None] * 6,
             "tendencies": [None] * 6, "events": [[9, 144, 60, 88]], "leds": "000000" * 52}
    raw = json.dumps(frame).encode() + b"\n"
    decoder = Decoder()
    assert decoder.feed(raw[:7]) == ([], 0, None)
    assert decoder.feed(raw[7:-1]) == ([], 0, None)
    assert decoder.feed(raw[-1:])[0] == [frame]
    assert decoder.feed(b"not-json\n")[1] == 1
    broken = dict(frame, support=[128] + [None] * 5)
    assert decoder.feed(json.dumps(broken).encode() + b"\n")[1] == 1
    for changes in ({"seq": True}, {"physical": 1 << 52}, {"leds": "zz" * 156},
                    {"answer": -1}, {"captured": [None] * 23},
                    {"requested": dict(prefs, colour=4)}, {"chord": {"root": 12, "shape": "Major", "tones": 145}}):
        assert decoder.feed(json.dumps(dict(frame, **changes)).encode() + b"\n")[1] == 1
    assert decoder.feed(b"x" * (MAX_LINE + 1))[1] == 1
    assert not decoder.buffer
    assert decoder.feed(b"ignored\n" + raw)[0] == [frame]
    assert note_name(60) == "C4" and note_name(0) == "C-1" and note_name(127) == "G9"
    assert chord_name({"root": 9, "shape": "Minor7"}) == "Am7"
    assert control_name(44, 1) == "remember" and control_name(49, 1) == "answer"
    assert control_name(44) == "pedal" and control_name(49) == "deceive"
    assert control_name(27) == "colour:Extended"
    assert control_name(34) == "foundation:Root"
    assert control_name(41) == "motion:Rising"
    assert control_name(48) == "reharm:Relative"
    typed_prefs = {"colour": "SecondsFourths", "foundation": "Seventh", "motion": "Outward",
                   "reharm": "AppliedDominant", "direction": "Deceive", "delay": True, "pedal": True}
    typed_frame = dict(frame, v=2, requested=typed_prefs, applied=typed_prefs, limited=15, cause=(1 << 6) | (1 << 7),
                       accepted=(1 << 44) | (1 << 49))
    del typed_frame["answer"]
    typed_raw = json.dumps(typed_frame).encode() + b"\n"
    assert decoder.feed(typed_raw[:13]) == ([], 0, None)
    assert decoder.feed(typed_raw[13:])[0] == [typed_frame]
    for direction in ("Explore", "Substitute"):
        directed = dict(typed_frame, requested=dict(typed_prefs, direction=direction))
        assert decoder.feed(json.dumps(directed).encode() + b"\n")[0] == [directed]
        assert decoder.feed(json.dumps(dict(frame, requested=dict(prefs, direction=direction))).encode() + b"\n")[1] == 1
    for changes in ({"limited": 16}, {"limited": True},
                    {"requested": dict(typed_prefs, colour=1)},
                    {"requested": dict(typed_prefs, foundation="Lowest")},
                    {"requested": dict(typed_prefs, motion="Random")},
                    {"requested": dict(typed_prefs, reharm="Anything")},
                    {"requested": dict(typed_prefs, pedal=1)}):
        assert decoder.feed(json.dumps(dict(typed_frame, **changes)).encode() + b"\n")[1] == 1
    assert decoder.feed(raw + typed_raw)[0] == [frame, typed_frame]
    rendered = StringIO()
    with redirect_stdout(rendered):
        Renderer().show(typed_frame, "test-time")
    assert "pedal+deceive" in rendered.getvalue()
    assert "pedal,deceive" in rendered.getvalue()
    assert "colour=SecondsFourths foundation=Seventh motion=Outward" in rendered.getvalue()
    assert "limited by the held notes or voice range: colour, foundation, motion, reharm" in rendered.getvalue()
    seq = Sequence()
    assert seq.observe(1, 1) is None and seq.observe(2, 2) is None
    assert "2 missing" in seq.observe(5, 3)
    assert seq.observe(5, 4) is None
    assert "reset" in seq.observe(0, 0)
    seq.previous = (0xFFFFFFFF, 1)
    assert seq.observe(0, 2) is None
    with patch.object(fcntl, "ioctl") as ioctl, patch.object(os, "close") as close:
        close_serial(7)
        ioctl.assert_called_once_with(7, termios.TIOCMBIC, struct.pack("I", termios.TIOCM_DTR))
        close.assert_called_once_with(7)
    with patch.object(fcntl, "ioctl", side_effect=OSError(errno.ENODEV, "disconnected")), patch.object(os, "close") as close:
        close_serial(7)
        close.assert_called_once_with(7)
    attrs = [0, 0, 0, 0, 0, 0, [0] * termios.NCCS]
    with patch.object(os, "open", return_value=7), patch.object(termios, "tcgetattr", return_value=attrs), \
            patch.object(termios, "tcsetattr"), patch.object(fcntl, "ioctl") as ioctl:
        assert open_serial("unused-test-port") == 7
        assert attrs[2] & termios.HUPCL
        ioctl.assert_called_once_with(7, termios.TIOCMBIS, struct.pack("I", termios.TIOCM_DTR))
    print("SELF-TEST PASS: v1/v2 typed frames, fragmented/invalid/oversized input, controls, limitations, sequence gaps/wrap, DTR cleanup", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", help="explicit serial device, required for generic USB modem names")
    parser.add_argument("--wait", action="store_true", help="wait for a port and reconnect until the duration ends")
    parser.add_argument("--seconds", type=float, default=1200, help="total capture duration including waiting, default 1200")
    parser.add_argument("--output", type=Path, help="JSONL path; default target/harmony-session-UTC.jsonl")
    parser.add_argument("--clashes", action="store_true", help="also list adjacent generated/literal semitones")
    parser.add_argument("--self-test", action="store_true", help="test parsing without touching serial hardware or files")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if not math.isfinite(args.seconds) or args.seconds <= 0:
        parser.error("--seconds must be a finite positive duration")
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output = args.output or Path(__file__).resolve().parents[3] / "target" / f"harmony-session-{stamp}.jsonl"
    output = output.expanduser()
    deadline = time.monotonic() + args.seconds
    fd = None
    captured = rejected_total = 0
    last_wait = last_warning = 0.0
    print(f"Harmony telemetry capture -> {output}\n"
          f"Duration {args.seconds:g}s; read-only serial input plus DTR; MIDI events are attempted firmware writes.", flush=True)
    try:
        output.parent.mkdir(parents=True, exist_ok=True)
        with output.open("a", encoding="utf-8", buffering=1) as recording:
            while time.monotonic() < deadline:
                if fd is None:
                    port = discover(args.port)
                    if port is None:
                        if not args.wait:
                            print("No matching Harmony serial port. Use --wait or an explicit --port; the telemetry firmware must be connected.", flush=True)
                            return 2
                        if time.monotonic() - last_wait >= 30:
                            print(f"Waiting for {args.port or 'a TECLA/HARMONY-named USB modem'}...", flush=True)
                            last_wait = time.monotonic()
                        time.sleep(min(0.5, max(0, deadline - time.monotonic())))
                        continue
                    try:
                        fd = open_serial(port)
                    except OSError as error:
                        if not args.wait:
                            raise
                        if time.monotonic() - last_warning >= 5:
                            print(f"Cannot open {port}: {error}; waiting to reconnect.", flush=True)
                            last_warning = time.monotonic()
                        time.sleep(min(0.5, max(0, deadline - time.monotonic())))
                        continue
                    decoder = Decoder()
                    renderer = Renderer(args.clashes)
                    print(f"CONNECTED {port}; DTR asserted; waiting for schema-v1/v2 frames.", flush=True)
                try:
                    readable, _, _ = select.select([fd], [], [], min(0.5, max(0, deadline - time.monotonic())))
                    if not readable:
                        continue
                    data = os.read(fd, 4096)
                    if not data:
                        raise OSError(errno.ENODEV, "serial device disconnected")
                except OSError as error:
                    if error.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                        continue
                    print(f"DISCONNECTED {port}: {error}", flush=True)
                    close_serial(fd)
                    fd = None
                    if not args.wait:
                        return 2
                    continue
                frames, rejected, last_error = decoder.feed(data)
                rejected_total += rejected
                if rejected and time.monotonic() - last_warning >= 5:
                    print(f"  ! rejected frame(s), total={rejected_total}: {last_error}", flush=True)
                    last_warning = time.monotonic()
                for frame in frames:
                    host_time = datetime.now(timezone.utc).isoformat(timespec="milliseconds")
                    record = dict(frame, host_time=host_time, host_monotonic_ns=time.monotonic_ns(), port=port)
                    recording.write(json.dumps(record, separators=(",", ":")) + "\n")
                    captured += 1
                    renderer.show(frame, host_time)
    except KeyboardInterrupt:
        print("Capture interrupted.", flush=True)
    except (OSError, ValueError) as error:
        print(f"Capture stopped: {error}", file=sys.stderr, flush=True)
        return 2
    finally:
        if fd is not None:
            close_serial(fd)
        print(f"DONE frames={captured} rejected={rejected_total} output={output}", flush=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
