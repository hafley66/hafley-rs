# python_module_resolve/main.py: every module-plane shape the resolve arms
# read. Named import, aliased import, star import, a type reference, and an
# `__all__`-excluded name that must bind nothing through the plane.

from pkg.mod import *
from pkg.mod import Widget
from pkg.mod import secret as sneaky


def main() -> None:
    exported()
    sneaky()
    secret()


class Panel:
    widget: Widget

    def __init__(self) -> None:
        self.widget = Widget()
