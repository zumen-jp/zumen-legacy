"""Checks the wheels and the sdist that maturin built (CI: .github/workflows/python-wheels.yml).

Every wheel and the sdist must carry the text of the license of zumen (LICENSE, MIT) as it is in the repository. The
bundled font (BIZ UDGothic, SIL Open Font License 1.1) is compiled into the extension module, and its license asks
every copy of the font to carry its text. So a wheel must hold the bytes of the font in its module and the texts of both
licenses in dist-info/licenses, and the sdist the font and the licenses that building it needs. A wheel must also be an
abi3 wheel (one file for every Python from 3.12) with a manylinux or macOS tag, never a bare linux_x86_64, unless
--any-platform accepts the tag of a wheel that was built on the machine itself.
"""

import argparse
import re
import sys
import tarfile
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FONT = (ROOT / "crates/zumen-render/fonts/BIZUDGothic-Regular.ttf").read_bytes()
LICENSE = (ROOT / "LICENSE").read_bytes()
FONT_LICENSE = (ROOT / "crates/zumen-render/fonts/OFL.txt").read_bytes()
WHEEL = re.compile(r"zumen-(?P<version>[^-]+)-cp312-abi3-(?P<platform>[\w.]+)\.whl")
RELEASE_PLATFORM = re.compile(r"(?:manylinux|macosx)_[\w.]+")
SDIST = re.compile(r"zumen-(?P<version>[^-]+)\.tar\.gz")


def check_wheel(path: Path, any_platform: bool) -> list[str]:
    """Returns what is wrong with a wheel; the list is empty when it is right."""
    match = WHEEL.fullmatch(path.name)
    if match is None:
        return [f"{path.name} is not named zumen-<version>-cp312-abi3-<platform>.whl"]
    if not any_platform and RELEASE_PLATFORM.fullmatch(match["platform"]) is None:
        return [f"{path.name} has the platform tag {match['platform']}, which is not manylinux or macOS"]
    with zipfile.ZipFile(path) as wheel:
        names = wheel.namelist()
        modules = [name for name in names if name.startswith("zumen/_zumen.") and name.endswith(".so")]
        if len(modules) != 1 or not modules[0].startswith("zumen/_zumen.abi3."):
            return [f"{path.name} should hold exactly one abi3 extension module, found {modules}"]
        errors = []
        if FONT not in wheel.read(modules[0]):
            errors.append(f"{modules[0]} does not contain the bytes of BIZUDGothic-Regular.ttf")
        licenses = f"zumen-{match['version']}.dist-info/licenses/"
        held = [wheel.read(name) for name in names if name.startswith(licenses)]
        if LICENSE not in held:
            errors.append(f"{licenses} does not hold the text of LICENSE, the license of zumen")
        if FONT_LICENSE not in held:
            errors.append(f"{licenses} does not hold the text of OFL.txt, the license of the bundled font")
    return errors


def check_sdist(path: Path) -> list[str]:
    """Returns what is wrong with an sdist; the list is empty when it is right."""
    match = SDIST.fullmatch(path.name)
    if match is None:
        return [f"{path.name} is not named zumen-<version>.tar.gz"]
    top = f"zumen-{match['version']}/"
    wanted = {
        f"{top}LICENSE": LICENSE,
        f"{top}crates/zumen-render/fonts/BIZUDGothic-Regular.ttf": FONT,
        f"{top}crates/zumen-render/fonts/OFL.txt": FONT_LICENSE,
        f"{top}licenses/BIZUDGothic-OFL.txt": FONT_LICENSE,
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("folder", type=Path, help="the folder with the wheels and the sdist")
    parser.add_argument("--any-platform", action="store_true", help="accept any platform tag in the name of a wheel")
    args = parser.parse_args()
    wheels = sorted(args.folder.glob("*.whl"))
    sdists = sorted(args.folder.glob("*.tar.gz"))
    if not wheels and not sdists:
        print(f"FAIL no wheel or sdist in {args.folder}")
        return 1
    failed = False
    for path, errors in [
        *((wheel, check_wheel(wheel, args.any_platform)) for wheel in wheels),
        *((sdist, check_sdist(sdist)) for sdist in sdists),
    ]:
        failed = failed or bool(errors)
        print(f"{'FAIL' if errors else 'ok  '} {path.name}")
        for error in errors:
            print(f"     {error}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
