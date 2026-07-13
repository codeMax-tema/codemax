from pathlib import Path

from PyInstaller.utils.hooks import collect_all

root = Path(SPECPATH)
datas = []
binaries = []
hiddenimports = []
for package in ("fastapi", "langgraph", "openai", "pydantic", "uvicorn"):
    package_datas, package_binaries, package_hiddenimports = collect_all(package)
    datas += package_datas
    binaries += package_binaries
    hiddenimports += package_hiddenimports

a = Analysis([str(root / "app" / "main.py")], pathex=[str(root)], binaries=binaries, datas=datas, hiddenimports=hiddenimports, noarchive=False)
pyz = PYZ(a.pure)
exe = EXE(pyz, a.scripts, a.binaries, a.datas, [], name="codemax-agent", console=False)
