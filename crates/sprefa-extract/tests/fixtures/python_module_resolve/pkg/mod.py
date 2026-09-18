# python_module_resolve/pkg/mod.py: the module plane's own declarations.
# `__all__` makes `secret` unbindable through any star re-export.

__all__ = ["exported"]

SECRET_KIND = "hidden"


def exported() -> int:
    return 1


def secret() -> int:
    return 2


class Widget:
    def area(self) -> int:
        return 3
