"""Exercise the installed wheel outside the source tree, including Unicode paths."""

import pathlib
import sys

import zumen

fixtures, directory = map(pathlib.Path, sys.argv[1:])
source = fixtures / "jww" / "minimal-v700.jww"
drawing = directory / "図面 01.jww"
drawing.write_bytes(source.read_bytes())
scene = zumen.read(drawing)
assert scene.format == "jww"
assert len(scene) > 0
assert scene.to_json() == zumen.read(source.read_bytes()).to_json()
assert scene.render_png(dpi=30).startswith(b"\x89PNG\r\n\x1a\n")
assert pathlib.Path(zumen.__file__).is_relative_to(directory)
print("Installed wheel: Unicode paths, scene JSON and PNG rendering passed.")
