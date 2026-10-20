# gasm_names=yes (detect.py): the engine's link without -s, which SCons adds to builds
# without debug symbols and which strips the wasm name section (function names).
#   python3 link_names.py <clang++> <link arguments...>
import os
import sys

os.execv(sys.argv[1], [sys.argv[1]] + [a for a in sys.argv[2:] if a != "-s"])
