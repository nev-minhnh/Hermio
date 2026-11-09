#!/usr/bin/env python3
"""Package a Windows build of Hermio into release zips.

    python scripts/package.py [--release-dir DIR] [--out DIR] [--no-source]

--release-dir  folder with Hermio.exe and bundle/nsis (default: the MSVC build,
               src-tauri/target/release, or the cross-build target if that one exists)
--out          where the zips go (default: dist-zips)
--no-source    skip the source zip (CI: GitHub already serves the source)
"""
import argparse, json, os, zipfile

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
V = json.load(open(os.path.join(root, "package.json"), encoding="utf-8"))["version"]

ap = argparse.ArgumentParser()
ap.add_argument("--release-dir")
ap.add_argument("--out", default=os.path.join(root, "dist-zips"))
ap.add_argument("--no-source", action="store_true")
args = ap.parse_args()

rel = args.release_dir
if not rel:
    cands = [os.path.join(root, "src-tauri", "target", "release"),
             os.path.join(root, "src-tauri", "target", "x86_64-pc-windows-gnu", "release")]
    rel = next((c for c in cands if os.path.isfile(os.path.join(c, "Hermio.exe"))), cands[0])
OUT = args.out
os.makedirs(OUT, exist_ok=True)
made = []

if not args.no_source:
    skip_dirs = {"node_modules", "dist", "dist-zips", "target", "gen", ".svelte-kit", ".git"}
    f = os.path.join(OUT, f"Hermio-{V}-source.zip")
    with zipfile.ZipFile(f, "w", zipfile.ZIP_DEFLATED) as z:
        for d, ds, fs in os.walk(root):
            ds[:] = [x for x in ds if x not in skip_dirs]
            for name in fs:
                if name == "secrets.local.env":
                    continue
                p = os.path.join(d, name)
                z.write(p, "hermio/" + os.path.relpath(p, root).replace(os.sep, "/"))
    made.append(f)

setup = os.path.join(rel, "bundle", "nsis", f"Hermio_{V}_x64-setup.exe")
f = os.path.join(OUT, f"Hermio-{V}-installer.zip")
with zipfile.ZipFile(f, "w", zipfile.ZIP_DEFLATED) as z:
    z.write(setup, f"Hermio_{V}_x64-setup.exe")
made.append(f)

readme = f"""Hermio {V} — portable
Hermio – Automate your inbox flow.

Run Hermio.exe. All data (mail index, settings, downloaded and POP3 mail, AI runtime) is
kept in Documents\\Hermio_Emails, created on the first start; data an older portable copy kept
in the "data" folder beside Hermio.exe is moved there automatically.
Passwords, tokens and API keys are stored in Windows Credential Manager.
Requires Microsoft Edge WebView2 Runtime (built into Windows 11).

Coming from Nelo? Settings -> Backup & transfer -> "Copy data from an older copy" and pick
the old Nelo folder. (The installed version moves Nelo's data over by itself.)

On-device AI (NPU): Settings -> AI assistant -> On-device AI (NPU).
- AMD Ryzen AI 300-series (XDNA 2): FastFlowLM runtime (https://github.com/FastFlowLM/FastFlowLM).
- Intel Core Ultra: OpenVINO Model Server (https://github.com/openvinotoolkit/model_server).

Source and licences (MIT or Apache-2.0): https://github.com/nev-minhnh/Hermio
"""
d = f"Hermio-{V}-portable"
f = os.path.join(OUT, f"{d}.zip")
with zipfile.ZipFile(f, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr(d + "/", "")
    z.writestr(d + "/README.txt", readme)
    loader = os.path.join(rel, "WebView2Loader.dll")
    if os.path.isfile(loader):  # only the MinGW cross-build needs it next to the exe
        z.write(loader, d + "/WebView2Loader.dll")
    z.write(os.path.join(rel, "Hermio.exe"), d + "/Hermio.exe")
    z.writestr(d + "/portable.txt", "Hermio portable mode: data is stored in the data folder next to Hermio.exe.\n")
    for lic in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"]:
        p = os.path.join(root, lic)
        if os.path.isfile(p):
            z.write(p, f"{d}/{lic}")
made.append(f)

for f in made:
    zz = zipfile.ZipFile(f)
    assert zz.testzip() is None
    assert not any("secrets.local" in x for x in zz.namelist())
    print(f, os.path.getsize(f))
