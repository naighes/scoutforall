#!/usr/bin/env python3
"""The DLLs a Windows executable loads at start must all come with Windows:
a missing one stops it before it runs ("libstdc++-6.dll not found", "the code
execution cannot proceed"), and the self update and the downloads ship the
bare executable. The runtimes of the compilers (libstdc++-6.dll,
libgcc_s_seh-1.dll, libwinpthread-1.dll of MinGW, VCRUNTIME140.dll of MSVC)
are linked into it (.cargo/config.toml).

    check-windows-dlls.py EXE...

Prints the DLLs of each executable, fails naming the ones not of Windows.
"""
import struct
import sys

# what Windows 10 and 11 ship, as the executables import it today; the
# api-ms-win-* sets are the API sets of Windows (the UCRT among them).
# Media Foundation (mf, mfplat, mfreadwrite) lacks on the N editions
# without the Media Feature Pack
WINDOWS = {
    "advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "combase.dll",
    "comctl32.dll", "comdlg32.dll", "crypt32.dll", "d3d11.dll", "d3d12.dll",
    "dwmapi.dll", "dxgi.dll", "gdi32.dll", "imm32.dll", "kernel32.dll",
    "ktmw32.dll", "mf.dll", "mfplat.dll", "mfreadwrite.dll", "ntdll.dll",
    "ole32.dll", "oleaut32.dll", "opengl32.dll", "propsys.dll", "rpcrt4.dll",
    "secur32.dll", "setupapi.dll", "shell32.dll", "shlwapi.dll", "user32.dll",
    "userenv.dll", "uxtheme.dll", "version.dll", "winmm.dll", "ws2_32.dll",
}


def imports(path):
    """The DLLs of the import table and of the delay-load one."""
    with open(path, "rb") as f:
        data = f.read()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[pe:pe + 4] != b"PE\0\0":
        raise ValueError(f"{path}: not a PE executable")
    sections = struct.unpack_from("<H", data, pe + 6)[0]
    optional_size = struct.unpack_from("<H", data, pe + 20)[0]
    optional = pe + 24
    magic = struct.unpack_from("<H", data, optional)[0]
    directories = optional + (112 if magic == 0x20B else 96)
    table = optional + optional_size
    spans = [struct.unpack_from("<IIII", data, table + 40 * i + 8)
             for i in range(sections)]

    def offset(rva):
        for size, address, _, raw in spans:
            if address <= rva < address + max(size, 1):
                return rva - address + raw
        raise ValueError(f"{path}: address {rva:#x} in no section")

    def name(rva):
        start = offset(rva)
        return data[start:data.index(b"\0", start)].decode("ascii")

    found = []
    # the import directory (1): 20 bytes each, the name at 12
    # the delay-load one (13): 32 bytes each, the name at 4
    for index, size, at in ((1, 20, 12), (13, 32, 4)):
        rva = struct.unpack_from("<I", data, directories + 8 * index)[0]
        if rva == 0:
            continue
        entry = offset(rva)
        while any(data[entry:entry + size]):
            found.append(name(struct.unpack_from("<I", data, entry + at)[0]))
            entry += size
    return found


def main(paths):
    if not paths:
        print(__doc__.strip())
        return 2
    failed = False
    for path in paths:
        dlls = sorted(set(d.lower() for d in imports(path)))
        foreign = [d for d in dlls
                   if d not in WINDOWS and not d.startswith("api-ms-win-")]
        print(f"{path}: {' '.join(dlls)}")
        if foreign:
            print(f"{path}: DLLs not of Windows: {' '.join(foreign)}")
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
