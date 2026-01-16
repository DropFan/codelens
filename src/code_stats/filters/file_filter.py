"""文件过滤功能"""

import fnmatch
from pathlib import Path
from typing import List, Optional, Pattern, Set

from code_stats.constants import (
    BINARY_EXTENSIONS,
    CODE_EXTENSIONS,
    DOC_EXTENSIONS,
    LANGUAGE_MAP,
)
from code_stats.filters.gitignore import GitIgnoreMatcher


# 特殊文件名（无扩展名但可能是脚本）
SPECIAL_FILES: Set[str] = {
    'Makefile', 'makefile', 'GNUmakefile', 'BSDmakefile',
    'Dockerfile', 'dockerfile', 'Containerfile',
    'Jenkinsfile', 'jenkinsfile',
    'Rakefile', 'rakefile',
    'Gemfile', 'Guardfile', 'Capfile', 'Thorfile',
    'Vagrantfile', 'Berksfile', 'Cheffile',
    'Pipfile', 'SConstruct', 'SConscript',
    'BUILD', 'BUILD.bazel', 'WORKSPACE',
    'CMakeLists.txt', 'meson.build',
    'Cargo.toml', 'go.mod', 'go.sum',
    'package.json', 'tsconfig.json',
    'pom.xml', 'build.gradle', 'build.gradle.kts',
    'requirements.txt', 'setup.py', 'setup.cfg',
    'composer.json', 'phpunit.xml',
    'Podfile', 'Package.swift',
    '.gitlab-ci.yml', '.travis.yml', '.circleci/config.yml',
    'azure-pipelines.yml', 'appveyor.yml',
    '.github/workflows/*.yml', '.github/workflows/*.yaml'
}


def is_binary_file(file_path: str) -> bool:
    """检测文件是否为二进制文件

    Args:
        file_path: 文件路径

    Returns:
        bool: True 如果是二进制文件
    """
    try:
        with open(file_path, 'rb') as f:
            # 读取前8192字节来判断
            chunk = f.read(8192)
            if not chunk:
                return False

            # 检测 NULL 字节
            if b'\x00' in chunk:
                return True

            # 检测非文本字符的比例
            text_chars = bytearray(
                {7, 8, 9, 10, 12, 13, 27} | set(range(0x20, 0x100)) - {0x7f}
            )
            non_text = len([b for b in chunk if b not in text_chars])

            # 如果非文本字符超过30%，认为是二进制文件
            if non_text / len(chunk) > 0.30:
                return True

        return False
    except Exception:
        return True  # 读取出错时假定为二进制文件


def is_code_file(
    file_path: str,
    *,
    include_all: bool = False,
    target_languages: Optional[Set[str]] = None,
    exclude_file_patterns: Optional[List[Pattern]] = None,
    include_file_patterns: Optional[List[Pattern]] = None,
    gitignore_matcher: Optional[GitIgnoreMatcher] = None,
) -> bool:
    """检查文件是否是代码文件

    Args:
        file_path: 文件路径
        include_all: 是否统计所有文件（但仍排除二进制）
        target_languages: 目标语言集合
        exclude_file_patterns: 排除文件的正则模式列表
        include_file_patterns: 包含文件的正则模式列表
        gitignore_matcher: GitIgnore 匹配器

    Returns:
        bool: True 如果是代码文件
    """
    file_name = Path(file_path).name
    file_ext = Path(file_path).suffix.lower()

    # 排除二进制文件
    if file_ext in BINARY_EXTENSIONS:
        return False

    # 检查 gitignore 规则
    if gitignore_matcher and gitignore_matcher.is_ignored(file_path, is_dir=False):
        return False

    # 检查文件排除正则表达式
    if exclude_file_patterns:
        for regex in exclude_file_patterns:
            if regex.search(file_path):
                return False

    # 如果指定了包含模式，则只处理匹配的文件
    if include_file_patterns:
        matched = False
        for regex in include_file_patterns:
            if regex.search(file_path):
                matched = True
                break
        if not matched:
            return False
        # 如果匹配了包含模式，且没有指定语言过滤，直接返回 True
        if not target_languages:
            return True
        # 如果同时指定了语言过滤，需要检查语言
        lang = LANGUAGE_MAP.get(file_ext, 'other')
        return lang in target_languages

    # 如果指定了统计所有文件（但仍排除二进制）
    if include_all:
        return True

    # 排除文档文件
    if file_ext in DOC_EXTENSIONS or file_name.lower() in DOC_EXTENSIONS:
        return False

    # 如果指定了语言过滤
    if target_languages:
        lang = LANGUAGE_MAP.get(file_ext, 'other')
        if lang not in target_languages:
            return False

    # 包含代码文件
    if file_ext in CODE_EXTENSIONS:
        return True

    # 检查没有扩展名但可能是脚本的文件
    if not file_ext and file_name in SPECIAL_FILES:
        return True

    # 检查特殊模式匹配
    for pattern in SPECIAL_FILES:
        if '*' in pattern and fnmatch.fnmatch(file_name, pattern):
            return True

    return False
