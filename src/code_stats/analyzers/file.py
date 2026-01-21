"""文件内容分析器"""

import os
import re
from pathlib import Path
from typing import Optional

from code_stats.analyzers.base import FileStats
from code_stats.analyzers.patterns import (
    get_complexity_keywords,
    get_function_pattern,
    get_multi_line_comment,
    get_single_line_comment,
)
from code_stats.constants import LANGUAGE_MAP
from code_stats.filters.file_filter import is_binary_file


class FileContentAnalyzer:
    """文件内容分析器"""

    def __init__(
        self,
        *,
        min_lines: Optional[int] = None,
        max_lines: Optional[int] = None,
        verbose: bool = False,
    ):
        """
        初始化文件分析器

        Args:
            min_lines: 最小行数过滤
            max_lines: 最大行数过滤
            verbose: 是否输出详细信息
        """
        self.min_lines = min_lines
        self.max_lines = max_lines
        self.verbose = verbose

    def analyze(self, file_path: str) -> FileStats:
        """分析文件内容，返回详细的行数统计和复杂度信息

        Args:
            file_path: 文件路径

        Returns:
            FileStats: 文件统计数据
        """
        # 先检测是否为二进制文件
        if is_binary_file(file_path):
            return FileStats.empty()

        try:
            file_size = os.path.getsize(file_path)
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()
                lines = content.split('\n')

            total_lines = len(lines)
            code_lines = 0
            comment_lines = 0
            blank_lines = 0

            # 复杂度分析变量
            functions = 0
            complexity = 1  # 基础复杂度为1
            max_depth = 0
            current_depth = 0

            # 获取文件扩展名以确定注释风格
            ext = Path(file_path).suffix.lower()
            lang = LANGUAGE_MAP.get(ext, 'other')

            # 获取语言特定的模式
            func_pattern_str = get_function_pattern(lang)
            func_pattern = re.compile(func_pattern_str) if func_pattern_str else None
            keywords = get_complexity_keywords(lang)

            # 获取注释符号
            single_comment = get_single_line_comment(lang)
            multi_comment = get_multi_line_comment(lang)

            in_multi_comment = False
            multi_start, multi_end = multi_comment if multi_comment else (None, None)

            for line in lines:
                stripped = line.strip()

                # 空行
                if not stripped:
                    blank_lines += 1
                    continue

                # 处理多行注释
                if multi_comment:
                    # Python 的特殊处理（docstring）
                    if lang == 'python' and '"""' in stripped:
                        if stripped.count('"""') == 2:
                            # 单行 docstring
                            comment_lines += 1
                            continue
                        else:
                            in_multi_comment = not in_multi_comment
                            comment_lines += 1
                            continue

                    # 其他语言的多行注释
                    if multi_start and multi_end:
                        if multi_start in stripped and multi_end in stripped:
                            # 单行内的多行注释
                            comment_lines += 1
                            continue
                        elif multi_start in stripped:
                            in_multi_comment = True
                            comment_lines += 1
                            continue
                        elif multi_end in stripped:
                            in_multi_comment = False
                            comment_lines += 1
                            continue
                        elif in_multi_comment:
                            comment_lines += 1
                            continue

                # 单行注释（确保 single_comment 非空，避免空字符串匹配所有行）
                if single_comment and stripped.startswith(single_comment):
                    comment_lines += 1
                else:
                    code_lines += 1
                    # 复杂度分析（仅对代码行）
                    if func_pattern and func_pattern.search(line):
                        functions += 1
                    for kw in keywords:
                        if kw in line:
                            complexity += 1
                    # 嵌套深度分析（基于缩进或大括号）
                    if lang == 'python':
                        indent = len(line) - len(line.lstrip())
                        depth = indent // 4  # 假设4空格缩进
                        max_depth = max(max_depth, depth)
                    else:
                        current_depth += line.count('{') - line.count('}')
                        max_depth = max(max_depth, current_depth)

            # 应用行数过滤
            if self.min_lines and total_lines < self.min_lines:
                return FileStats.empty()

            if self.max_lines and total_lines > self.max_lines:
                return FileStats.empty()

            return FileStats(
                total=total_lines,
                code=code_lines,
                comment=comment_lines,
                blank=blank_lines,
                size=file_size,
                functions=functions,
                complexity=complexity,
                max_depth=max_depth
            )

        except Exception as e:
            if self.verbose:
                print(f"Error reading {file_path}: {e}")
            return FileStats.empty()

    def count_lines(self, file_path: str) -> int:
        """统计文件行数（向后兼容）

        Args:
            file_path: 文件路径

        Returns:
            int: 文件总行数
        """
        result = self.analyze(file_path)
        return result.total
