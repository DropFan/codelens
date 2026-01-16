"""目录过滤和项目类型检测"""

import fnmatch
import os
from collections import defaultdict
from pathlib import Path
from typing import List, Optional, Pattern, Set

from code_stats.constants import (
    BASE_EXCLUDE_DIRS,
    BINARY_EXTENSIONS,
    CONFIG_EXTENSIONS,
    LANGUAGE_EXCLUDE_DIRS,
    PROJECT_MARKERS,
)
from code_stats.filters.gitignore import GitIgnoreMatcher


def detect_project_types(repo_path: str) -> Set[str]:
    """检测仓库中存在的项目类型/语言

    Args:
        repo_path: 仓库路径

    Returns:
        set: 检测到的语言/框架类型集合
    """
    detected_types: Set[str] = set()

    try:
        # 检查根目录的标记文件
        for lang, markers in PROJECT_MARKERS.items():
            for marker in markers:
                if '*' in marker:
                    # 通配符模式，检查匹配的文件
                    for item in os.listdir(repo_path):
                        if fnmatch.fnmatch(item, marker):
                            detected_types.add(lang)
                            break
                else:
                    # 精确匹配
                    marker_path = os.path.join(repo_path, marker)
                    if os.path.exists(marker_path):
                        detected_types.add(lang)
                        break

        # 如果没有检测到，扫描文件扩展名来推断
        if not detected_types:
            ext_count: defaultdict = defaultdict(int)
            scan_limit = 100  # 只扫描前100个文件
            file_count = 0

            for root, dirs, files in os.walk(repo_path):
                # 跳过隐藏目录和基础排除目录
                dirs[:] = [
                    d for d in dirs
                    if not d.startswith('.') and d not in BASE_EXCLUDE_DIRS
                ]

                for file in files:
                    if file_count >= scan_limit:
                        break
                    ext = Path(file).suffix.lower()
                    if ext and ext not in BINARY_EXTENSIONS and ext not in CONFIG_EXTENSIONS:
                        ext_count[ext] += 1
                    file_count += 1

                if file_count >= scan_limit:
                    break

            # 根据文件扩展名推断语言
            ext_to_lang = {
                '.py': 'python', '.pyw': 'python', '.pyx': 'python',
                '.js': 'javascript', '.mjs': 'javascript', '.cjs': 'javascript',
                '.jsx': 'javascript',
                '.ts': 'typescript', '.tsx': 'typescript', '.mts': 'typescript',
                '.go': 'go',
                '.java': 'java',
                '.kt': 'kotlin', '.kts': 'kotlin',
                '.scala': 'scala', '.sc': 'scala',
                '.cs': 'csharp',
                '.fs': 'fsharp', '.fsx': 'fsharp',
                '.rb': 'ruby', '.rake': 'ruby',
                '.php': 'php',
                '.rs': 'rust',
                '.dart': 'dart',
                '.swift': 'swift',
                '.m': 'objc', '.mm': 'objc',
                '.c': 'c', '.h': 'c',
                '.cpp': 'cpp', '.cc': 'cpp', '.cxx': 'cpp',
                '.hpp': 'cpp', '.hxx': 'cpp',
                '.ex': 'elixir', '.exs': 'elixir',
                '.erl': 'erlang', '.hrl': 'erlang',
                '.hs': 'haskell', '.lhs': 'haskell',
                '.lua': 'lua',
                '.pl': 'perl', '.pm': 'perl',
                '.r': 'r', '.R': 'r',
                '.jl': 'julia',
                '.clj': 'clojure', '.cljs': 'clojure', '.cljc': 'clojure',
            }

            for ext, count in ext_count.items():
                if count >= 3 and ext in ext_to_lang:  # 至少3个同类型文件
                    detected_types.add(ext_to_lang[ext])

    except OSError:
        pass

    return detected_types


def build_exclude_dirs(
    repo_path: Optional[str] = None,
    *,
    smart_exclude: bool = True,
    verbose: bool = False,
) -> Set[str]:
    """根据项目类型动态构建排除目录列表

    Args:
        repo_path: 仓库路径，用于检测项目类型。如果为 None，则返回基础排除目录
        smart_exclude: 是否使用智能排除（基于检测到的项目类型）
        verbose: 是否输出详细信息

    Returns:
        set: 排除目录集合
    """
    # 始终包含基础排除目录
    exclude_dirs = BASE_EXCLUDE_DIRS.copy()

    if repo_path and smart_exclude:
        # 检测项目类型
        detected_types = detect_project_types(repo_path)

        if detected_types:
            if verbose:
                print(f"  检测到项目类型: {', '.join(sorted(detected_types))}")

            # 添加检测到的语言对应的排除目录
            for lang in detected_types:
                if lang in LANGUAGE_EXCLUDE_DIRS:
                    exclude_dirs.update(LANGUAGE_EXCLUDE_DIRS[lang])
        else:
            # 未检测到特定类型，使用所有语言的排除目录
            if verbose:
                print("  未检测到特定项目类型，使用完整排除列表")
            for lang_excludes in LANGUAGE_EXCLUDE_DIRS.values():
                exclude_dirs.update(lang_excludes)
    elif not smart_exclude:
        # 不使用智能排除，使用完整排除列表
        for lang_excludes in LANGUAGE_EXCLUDE_DIRS.values():
            exclude_dirs.update(lang_excludes)

    return exclude_dirs


def should_exclude_dir(
    dir_path: str,
    exclude_dirs: Set[str],
    *,
    include_all: bool = False,
    exclude_dir_patterns: Optional[List[Pattern]] = None,
    gitignore_matcher: Optional[GitIgnoreMatcher] = None,
) -> bool:
    """检查目录是否应该被排除

    Args:
        dir_path: 目录路径
        exclude_dirs: 排除目录集合
        include_all: 是否包含所有目录
        exclude_dir_patterns: 排除目录的正则模式列表
        gitignore_matcher: GitIgnore 匹配器

    Returns:
        bool: True 如果目录应该被排除
    """
    if include_all:
        return False

    dir_name = os.path.basename(dir_path)

    # 检查完全匹配
    if dir_name in exclude_dirs:
        return True

    # 检查模式匹配
    for pattern in exclude_dirs:
        if '*' in pattern or '?' in pattern:
            if fnmatch.fnmatch(dir_name, pattern):
                return True

    # 检查正则表达式匹配
    if exclude_dir_patterns:
        for regex in exclude_dir_patterns:
            if regex.search(dir_path):
                return True

    # 检查 gitignore 规则
    if gitignore_matcher and gitignore_matcher.is_ignored(dir_path, is_dir=True):
        return True

    return False
