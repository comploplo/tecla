$firmware?=@{{FIRMWARE_ELF}}

mach create "planck"
machine LoadPlatformDescription @{{REPL_PATH}}

sysbus LoadELF $firmware

cpu VectorTableOffset {{VTOR}}
cpu SP {{SP}}
cpu PC {{PC}}
cpu AddHook {{HOOK_ADDR}} "self.Log(LogLevel.Warning, 'HIL OK')"

start
emulation RunFor "00:00:05"
quit
