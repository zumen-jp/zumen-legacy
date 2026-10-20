"""Checks the wheels and the sdist that maturin built (CI: .github/workflows/python-wheels.yml).

    python bindings/python/scripts/check_dist.py <folder with the wheels and the sdist>

The bundled font (BIZ UDGothic, SIL Open Font License 1.1) is compiled into the extension module, and the license asks
every copy of the font to carry its text. So a wheel must hold the bytes of the font in its module and the text of the
license in dist-info/licenses, and the sdist the font and the license that building it needs. A wheel must also be an
abi3 wheel (one file for every Python from 3.10) with a manylinux or macOS tag, never a bare linux_x86_64.
"""

import re
import sys
import tarfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FONT = (ROOT / "crates/zumen-render/fonts/BIZUDGothic-Regular.ttf").read_bytes()
LICENSE = (ROOT / "crates/zumen-render/fonts/OFL.txt").read_bytes()
WHEEL = re.compile(r"zumen-(?P<version>[^-]+)-cp310-abi3-(?P<platform>(?:manylinux|macosx)_[\w.]+)\.whl")
SDIST = re.compile(r"zumen-(?P<version>[^-]+)\.tar\.gz")


def check_wheel(path: Path) -> list[str]:
    """Returns what is wrong with a wheel; the list is empty when it is right."""
    match = WHEEL.fullmatch(path.name)
    if match is None:
        return [f"{path.name} is not named zumen-<version>-cp310-abi3-<manylinux or macOS tag>.whl"]
    with zipfile.ZipFile(path) as wheel:
        names = wheel.namelist()
        modules = [name for name in names if name.startswith("zumen/_zumen.") and name.endswith(".so")]
        if len(modules) != 1 or not modules[0].startswith("zumen/_zumen.abi3."):
            return [f"{path.name} should hold exactly one abi3 extension module, found {modules}"]
        errors = []
        if FONT not in wheel.read(modules[0]):
            errors.append(f"{modules[0]} does not contain the bytes of BIZUDGothic-Regular.ttf")
        licenses = f"zumen-{match['version']}.dist-info/licenses/"
        if not any(name.startswith(licenses) and wheel.read(name) == LICENSE for name in names):
            errors.append(f"{licenses} does not hold the text of OFL.txt, the license of the bundled font")
    return errors


def check_sdist(path: Path) -> list[str]:
    """Returns what is wrong with an sdist; the list is empty when it is right."""
    match = SDIST.fullmatch(path.name)
    if match is None:
        return [f"{path.name} is not named zumen-<version>.tar.gz"]
    top = f"zumen-{match['version']}/"
    wanted = {
        f"{top}crates/zumen-render/fonts/BIZUDGothic-Regular.ttf": FONT,
        f"{top}crates/zumen-render/fonts/OFL.txt": LICENSE,
        f"{top}licenses/BIZUDGothic-OFL.txt": LICENSE,
    }
    errors = []
    with tarfile.open(path) as sdist:
        names = sdist.getnames()
        if f"{top}Cargo.lock" not in names:
            errors.append(f"{top}Cargo.lock is missing, so the build could not be locked")
        for name, content in wanted.items():
            member = sdist.extractfile(name) if name in names else None
            if member is None or member.read() != content:
                errors.append(f"{name} is missing or differs from the file in the repository")
    return errors


def main(folder: Path) -> int:
    wheels = sorted(folder.glob("*.whl"))
    sdists = sorted(folder.glob("*.tar.gz"))
    if not wheels and not sdists:
        print(f"FAIL no wheel or sdist in {folder}")
        return 1
    failed = False
    for path, check in [*((wheel, check_wheel) for wheel in wheels), *((sdist, check_sdist) for sdist in sdists)]:
        errors = check(path)
        failed = failed or bool(errors)
        print(f"{'FAIL' if errors else 'ok  '} {path.name}")
        for error in errors:
            print(f"     {error}")
    return 1 if failed else 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    sys.exit(main(Path(sys.argv[1])))
