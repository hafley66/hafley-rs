# python_modules/app/helpers.py: reached through `from .helpers import *`.
# The `__all__` list makes `internal` a declared-but-not-exported name: a star
# import of this module binds `helper` and `VALUE`, never `internal`.

__all__ = ["helper", "VALUE"]

VALUE = 1


def helper() -> int:
    return VALUE


def internal() -> int:
    return 0
