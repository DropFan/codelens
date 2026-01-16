"""仓库分析器"""

import os
from collections import defaultdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Pattern, Set

from code_stats.analyzers.file import FileContentAnalyzer
from code_stats.constants import DOC_EXTENSIONS
from code_stats.filters.dir_filter import build_exclude_dirs, should_exclude_dir
from code_stats.filters.file_filter import is_code_file
from code_stats.filters.gitignore import GitIgnoreMatcher


class RepositoryAnalyzer:
    """仓库分析器"""

    def __init__(
        self,
        *,
        include_all: bool = False,
        target_languages: Optional[Set[str]] = None,
        exclude_file_patterns: Optional[List[Pattern]] = None,
        include_file_patterns: Optional[List[Pattern]] = None,
        exclude_dir_patterns: Optional[List[Pattern]] = None,
        user_exclude_dirs: Optional[Set[str]] = None,
        max_depth: int = 0,
        min_lines: Optional[int] = None,
        max_lines: Optional[int] = None,
        use_gitignore: bool = True,
        smart_exclude: bool = True,
        verbose: bool = False,
        quiet: bool = False,
    ):
        """
        初始化仓库分析器

        Args:
            include_all: 统计所有文件
            target_languages: 目标语言集合
            exclude_file_patterns: 排除文件的正则模式
            include_file_patterns: 包含文件的正则模式
            exclude_dir_patterns: 排除目录的正则模式
            user_exclude_dirs: 用户指定的排除目录
            max_depth: 最大扫描深度（0表示无限制）
            min_lines: 最小行数过滤
            max_lines: 最大行数过滤
            use_gitignore: 是否使用 gitignore 规则
            smart_exclude: 是否使用智能排除
            verbose: 详细输出
            quiet: 安静模式
        """
        self.include_all = include_all
        self.target_languages = target_languages
        self.exclude_file_patterns = exclude_file_patterns or []
        self.include_file_patterns = include_file_patterns or []
        self.exclude_dir_patterns = exclude_dir_patterns or []
        self.user_exclude_dirs = user_exclude_dirs or set()
        self.max_depth = max_depth
        self.use_gitignore = use_gitignore
        self.smart_exclude = smart_exclude
        self.verbose = verbose
        self.quiet = quiet

        # 文件分析器
        self.file_analyzer = FileContentAnalyzer(
            min_lines=min_lines,
            max_lines=max_lines,
            verbose=verbose,
        )

        # 运行时状态
        self.exclude_dirs: Set[str] = set()
        self.gitignore_matcher: Optional[GitIgnoreMatcher] = None

    def analyze(self, repo_path: str) -> Dict[str, Any]:
        """分析单个仓库的代码统计

        Args:
            repo_path: 仓库路径

        Returns:
            dict: 仓库统计数据
        """
        # 动态构建排除目录列表
        self.exclude_dirs = build_exclude_dirs(
            repo_path,
            smart_exclude=self.smart_exclude,
            verbose=self.verbose,
        )
        # 添加用户指定的额外排除目录
        self.exclude_dirs.update(self.user_exclude_dirs)

        # 初始化 gitignore matcher
        if self.use_gitignore:
            self.gitignore_matcher = GitIgnoreMatcher(repo_path)
            if self.gitignore_matcher.enabled and not self.quiet:
                print("  已加载 .gitignore 规则")
        else:
            self.gitignore_matcher = None

        stats: Dict[str, Dict[str, int]] = defaultdict(lambda: {
            'files': 0, 'lines': 0, 'code_lines': 0,
            'comment_lines': 0, 'blank_lines': 0, 'size': 0
        })
        doc_stats: Dict[str, Dict[str, int]] = defaultdict(lambda: {
            'files': 0, 'lines': 0, 'size': 0
        })

        total_files = 0
        total_lines = 0
        total_code_lines = 0
        total_comment_lines = 0
        total_blank_lines = 0
        total_size = 0
        total_doc_files = 0
        total_doc_lines = 0
        total_doc_size = 0
        total_all_size = 0  # 仓库总大小
        file_details: List[Dict[str, Any]] = []

        # 文件大小分布统计
        size_distribution = {
            'tiny': 0,      # < 1KB
            'small': 0,     # 1KB - 10KB
            'medium': 0,    # 10KB - 100KB
            'large': 0,     # 100KB - 1MB
            'huge': 0       # > 1MB
        }

        # 代码复杂度统计
        total_functions = 0
        total_complexity = 0
        max_depth = 0

        for root, dirs, files in os.walk(repo_path):
            # 计算当前深度
            current_depth = root.replace(repo_path, '').count(os.sep)

            # 检查深度限制
            if self.max_depth > 0 and current_depth >= self.max_depth:
                dirs[:] = []  # 不再进入子目录
                continue

            # 过滤掉需要排除的目录
            dirs[:] = [
                d for d in dirs
                if not should_exclude_dir(
                    os.path.join(root, d),
                    self.exclude_dirs,
                    include_all=self.include_all,
                    exclude_dir_patterns=self.exclude_dir_patterns,
                    gitignore_matcher=self.gitignore_matcher,
                )
            ]

            for file in files:
                file_path = os.path.join(root, file)

                # 统计仓库总大小（所有文件）
                if os.path.isfile(file_path):
                    try:
                        total_all_size += os.path.getsize(file_path)
                    except OSError:
                        pass

                # 判断是否是文档文件
                file_ext = Path(file_path).suffix.lower()
                file_name = os.path.basename(file_path).lower()

                # 检查 gitignore 规则（用于文档文件）
                if (self.gitignore_matcher and
                        self.gitignore_matcher.is_ignored(file_path, is_dir=False)):
                    continue

                # 如果用户指定了 include_file_patterns，优先检查是否匹配
                is_explicit_include = False
                if self.include_file_patterns:
                    for regex in self.include_file_patterns:
                        if regex.search(file_path):
                            is_explicit_include = True
                            break

                # 如果不是显式包含的文件，且是文档扩展名，则作为文档统计
                if (not is_explicit_include and
                        (file_ext in DOC_EXTENSIONS or file_name in DOC_EXTENSIONS)):
                    # 统计文档文件
                    try:
                        file_size = os.path.getsize(file_path)
                        with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                            lines = len(f.readlines())

                        ext = file_ext or file_name
                        doc_stats[ext]['files'] += 1
                        doc_stats[ext]['lines'] += lines
                        doc_stats[ext]['size'] += file_size

                        total_doc_files += 1
                        total_doc_lines += lines
                        total_doc_size += file_size
                    except OSError:
                        pass

                elif is_code_file(
                    file_path,
                    include_all=self.include_all,
                    target_languages=self.target_languages,
                    exclude_file_patterns=self.exclude_file_patterns,
                    include_file_patterns=self.include_file_patterns,
                    gitignore_matcher=self.gitignore_matcher,
                ):
                    # 统计代码文件
                    file_stats = self.file_analyzer.analyze(file_path)
                    if file_stats.total > 0:
                        ext = Path(file_path).suffix.lower() or 'no_extension'
                        stats[ext]['files'] += 1
                        stats[ext]['lines'] += file_stats.total
                        stats[ext]['code_lines'] += file_stats.code
                        stats[ext]['comment_lines'] += file_stats.comment
                        stats[ext]['blank_lines'] += file_stats.blank
                        stats[ext]['size'] += file_stats.size

                        total_files += 1
                        total_lines += file_stats.total
                        total_code_lines += file_stats.code
                        total_comment_lines += file_stats.comment
                        total_blank_lines += file_stats.blank
                        total_size += file_stats.size

                        # 更新文件大小分布
                        fsize = file_stats.size
                        if fsize < 1024:
                            size_distribution['tiny'] += 1
                        elif fsize < 10 * 1024:
                            size_distribution['small'] += 1
                        elif fsize < 100 * 1024:
                            size_distribution['medium'] += 1
                        elif fsize < 1024 * 1024:
                            size_distribution['large'] += 1
                        else:
                            size_distribution['huge'] += 1

                        # 更新复杂度统计
                        total_functions += file_stats.functions
                        total_complexity += file_stats.complexity
                        max_depth = max(max_depth, file_stats.max_depth)

                        if self.verbose:
                            relative_path = os.path.relpath(file_path, repo_path)
                            file_details.append({
                                'path': relative_path,
                                'lines': file_stats.total,
                                'code_lines': file_stats.code,
                                'comment_lines': file_stats.comment,
                                'blank_lines': file_stats.blank,
                                'size': file_stats.size,
                                'extension': ext
                            })

        # 计算平均复杂度
        avg_complexity = total_complexity / total_files if total_files > 0 else 0
        avg_func_lines = total_code_lines / total_functions if total_functions > 0 else 0

        return {
            'total_files': total_files,
            'total_lines': total_lines,
            'total_code_lines': total_code_lines,
            'total_comment_lines': total_comment_lines,
            'total_blank_lines': total_blank_lines,
            'total_size': total_size,
            'doc_files': total_doc_files,
            'doc_lines': total_doc_lines,
            'doc_size': total_doc_size,
            'total_all_size': total_all_size,
            'by_extension': dict(stats),
            'doc_details': dict(doc_stats),
            'file_details': file_details if self.verbose else [],
            'size_distribution': size_distribution,
            'complexity': {
                'functions': total_functions,
                'total_complexity': total_complexity,
                'avg_complexity': round(avg_complexity, 2),
                'max_depth': max_depth,
                'avg_func_lines': round(avg_func_lines, 1)
            }
        }
