"""常量定义模块"""

from code_stats.constants.excludes import (
    BASE_EXCLUDE_DIRS,
    LANGUAGE_EXCLUDE_DIRS,
    DEFAULT_EXCLUDE_DIRS,
)
from code_stats.constants.extensions import (
    DOC_EXTENSIONS,
    CONFIG_EXTENSIONS,
    BINARY_EXTENSIONS,
    CODE_EXTENSIONS,
)
from code_stats.constants.languages import (
    PROJECT_MARKERS,
    LANGUAGE_MAP,
)

__all__ = [
    "BASE_EXCLUDE_DIRS",
    "LANGUAGE_EXCLUDE_DIRS",
    "DEFAULT_EXCLUDE_DIRS",
    "DOC_EXTENSIONS",
    "CONFIG_EXTENSIONS",
    "BINARY_EXTENSIONS",
    "CODE_EXTENSIONS",
    "PROJECT_MARKERS",
    "LANGUAGE_MAP",
]
