"""核心统计类"""

import concurrent.futures
import os
import re
import time
from collections import defaultdict
from typing import Any, Dict, List, Optional, Set

from code_stats.analyzers.repository import RepositoryAnalyzer
from code_stats.constants import LANGUAGE_MAP
from code_stats.git.info import GitInfoProvider
from code_stats.outputs import (
    ConsoleOutput,
    CsvOutput,
    HtmlOutput,
    JsonOutput,
    MarkdownOutput,
)


class CodeStatistics:
    """代码统计主类"""

    def __init__(self, args):
        """
        初始化代码统计器

        Args:
            args: 命令行参数
        """
        self.args = args
        self.current_dir = os.getcwd()
        self.progress_count = 0
        self.total_repos = 0

        # 处理额外的排除模式
        self.user_exclude_dirs: Set[str] = set()
        if args.excludes:
            for pattern in args.excludes.split(','):
                self.user_exclude_dirs.add(pattern.strip())

        # 处理语言过滤
        self.target_languages: Optional[Set[str]] = None
        if args.lang:
            self.target_languages = set(
                lang.strip().lower() for lang in args.lang.split(',')
            )

        # 处理正则表达式排除模式
        self.exclude_file_patterns = []
        if args.exclude_files:
            for pattern in args.exclude_files.split(','):
                try:
                    self.exclude_file_patterns.append(re.compile(pattern.strip()))
                except re.error as e:
                    print(f"警告：无效的文件排除正则表达式 '{pattern}': {e}")

        self.exclude_dir_patterns = []
        if args.exclude_dirs:
            for pattern in args.exclude_dirs.split(','):
                try:
                    self.exclude_dir_patterns.append(re.compile(pattern.strip()))
                except re.error as e:
                    print(f"警告：无效的目录排除正则表达式 '{pattern}': {e}")

        # 处理文件包含模式
        self.include_file_patterns = []
        if getattr(args, 'include_files', None):
            for pattern in args.include_files.split(','):
                try:
                    self.include_file_patterns.append(re.compile(pattern.strip()))
                except re.error as e:
                    print(f"警告：无效的文件包含正则表达式 '{pattern}': {e}")

        # 处理深度限制
        self.max_depth = getattr(args, 'depth', None) or 0

        # Git 信息选项
        self.git_info = getattr(args, 'git_info', False)
        self.git_provider = GitInfoProvider(verbose=getattr(args, 'verbose', False))

        # Gitignore 过滤选项
        self.use_gitignore = not getattr(args, 'no_gitignore', False)

        # 智能排除选项
        self.smart_exclude = not getattr(args, 'no_smart_exclude', False)

    def detect_language(self, repo_stats: Dict[str, Any]) -> str:
        """根据文件扩展名检测主要编程语言

        Args:
            repo_stats: 仓库统计数据

        Returns:
            str: 主要编程语言
        """
        language_lines: Dict[str, int] = defaultdict(int)
        for ext, data in repo_stats['by_extension'].items():
            lang = LANGUAGE_MAP.get(ext, 'other')
            language_lines[lang] += data['lines']

        if language_lines:
            return max(language_lines.items(), key=lambda x: x[1])[0]
        return 'unknown'

    def print_progress(self, message: str) -> None:
        """打印进度信息"""
        if self.args.verbose:
            self.progress_count += 1
            print(f"[{self.progress_count}/{self.total_repos}] {message}")

    def analyze_all_repos(self) -> List[Dict[str, Any]]:
        """分析所有仓库

        Returns:
            list: 所有仓库的统计数据
        """
        all_repos: List[Dict[str, Any]] = []
        repo_paths: List[tuple] = []

        # 收集所有仓库路径
        if self.args.dirs:
            for dir_name in self.args.dirs:
                # 支持绝对路径和相对路径
                if os.path.isabs(dir_name):
                    dir_path = dir_name
                else:
                    dir_path = os.path.join(self.current_dir, dir_name)

                if os.path.isdir(dir_path):
                    repo_name = os.path.basename(dir_path)
                    repo_paths.append((repo_name, dir_path))
                else:
                    print(f"警告：目录不存在或不是目录: {dir_path}")
        else:
            # 原有逻辑：扫描当前目录下的所有仓库
            items = os.listdir(self.current_dir)

            # 如果指定了特定仓库
            if self.args.repo:
                target_repos = set(repo.strip() for repo in self.args.repo.split(','))
                items = [item for item in items if item in target_repos]

            for item in sorted(items):
                item_path = os.path.join(self.current_dir, item)

                # 跳过非目录和以.开头的隐藏目录
                if not os.path.isdir(item_path) or item.startswith('.'):
                    continue

                # 跳过非仓库目录
                if not os.path.exists(os.path.join(item_path, '.git')):
                    continue

                repo_paths.append((item, item_path))

        self.total_repos = len(repo_paths)

        # 创建仓库分析器
        def create_analyzer():
            return RepositoryAnalyzer(
                include_all=self.args.all,
                target_languages=self.target_languages,
                exclude_file_patterns=self.exclude_file_patterns,
                include_file_patterns=self.include_file_patterns,
                exclude_dir_patterns=self.exclude_dir_patterns,
                user_exclude_dirs=self.user_exclude_dirs,
                max_depth=self.max_depth,
                min_lines=getattr(self.args, 'min_lines', None),
                max_lines=getattr(self.args, 'max_lines', None),
                use_gitignore=self.use_gitignore,
                smart_exclude=self.smart_exclude,
                verbose=getattr(self.args, 'verbose', False),
                quiet=getattr(self.args, 'quiet', False),
            )

        def analyze_repo(name: str, path: str) -> Optional[Dict[str, Any]]:
            """分析单个仓库"""
            analyzer = create_analyzer()
            stats = analyzer.analyze(path)

            if stats['total_files'] > 0:
                main_language = self.detect_language(stats)
                repo_info: Dict[str, Any] = {
                    'name': name,
                    'path': path,
                    'language': main_language,
                    'files': stats['total_files'],
                    'lines': stats['total_lines'],
                    'code_lines': stats['total_code_lines'],
                    'comment_lines': stats['total_comment_lines'],
                    'blank_lines': stats['total_blank_lines'],
                    'size': stats['total_size'],
                    'doc_files': stats.get('doc_files', 0),
                    'doc_lines': stats.get('doc_lines', 0),
                    'doc_size': stats.get('doc_size', 0),
                    'total_size': stats.get('total_all_size', stats['total_size']),
                    'details': stats['by_extension'],
                    'doc_details': stats.get('doc_details', {}),
                    'file_details': stats.get('file_details', []),
                    'size_distribution': stats.get('size_distribution', {}),
                    'complexity': stats.get('complexity', {})
                }
                # 添加 Git 信息
                if self.git_info:
                    repo_info['git'] = self.git_provider.get_info(path)
                return repo_info
            return None

        # 使用多线程并行处理
        if self.args.parallel and len(repo_paths) > 1:
            with concurrent.futures.ThreadPoolExecutor(
                max_workers=os.cpu_count()
            ) as executor:
                future_to_repo = {
                    executor.submit(analyze_repo, name, path): (name, path)
                    for name, path in repo_paths
                }

                for future in concurrent.futures.as_completed(future_to_repo):
                    name, path = future_to_repo[future]
                    self.print_progress(f"分析仓库: {name}")

                    try:
                        result = future.result()
                        if result:
                            all_repos.append(result)
                    except Exception as e:
                        print(f"Error analyzing {name}: {e}")
        else:
            # 串行处理
            for name, path in repo_paths:
                self.print_progress(f"分析仓库: {name}")
                result = analyze_repo(name, path)
                if result:
                    all_repos.append(result)

        return all_repos

    def generate_summary(self, all_repos: List[Dict[str, Any]]) -> Dict[str, Any]:
        """生成统计摘要

        Args:
            all_repos: 所有仓库的统计数据

        Returns:
            dict: 统计摘要
        """
        total_all_files = sum(repo['files'] for repo in all_repos)
        total_all_lines = sum(repo['lines'] for repo in all_repos)
        total_all_code_lines = sum(
            repo.get('code_lines', repo['lines']) for repo in all_repos
        )
        total_all_comment_lines = sum(repo.get('comment_lines', 0) for repo in all_repos)
        total_all_blank_lines = sum(repo.get('blank_lines', 0) for repo in all_repos)
        total_all_size = sum(repo.get('size', 0) for repo in all_repos)

        # 文档统计
        total_all_doc_files = sum(repo.get('doc_files', 0) for repo in all_repos)
        total_all_doc_lines = sum(repo.get('doc_lines', 0) for repo in all_repos)
        total_all_doc_size = sum(repo.get('doc_size', 0) for repo in all_repos)
        total_all_total_size = sum(
            repo.get('total_size', repo.get('size', 0)) for repo in all_repos
        )

        # 汇总代码详情（按扩展名）
        code_details_summary: Dict[str, Dict[str, int]] = defaultdict(
            lambda: {
                'files': 0, 'lines': 0, 'code_lines': 0,
                'comment_lines': 0, 'blank_lines': 0, 'size': 0
            }
        )
        for repo in all_repos:
            for ext, details in repo.get('details', {}).items():
                code_details_summary[ext]['files'] += details.get('files', 0)
                code_details_summary[ext]['lines'] += details.get('lines', 0)
                code_details_summary[ext]['code_lines'] += details.get('code_lines', 0)
                code_details_summary[ext]['comment_lines'] += details.get('comment_lines', 0)
                code_details_summary[ext]['blank_lines'] += details.get('blank_lines', 0)
                code_details_summary[ext]['size'] += details.get('size', 0)

        # 汇总文档详情（按扩展名）
        doc_details_summary: Dict[str, Dict[str, int]] = defaultdict(
            lambda: {'files': 0, 'lines': 0, 'size': 0}
        )
        for repo in all_repos:
            for ext, details in repo.get('doc_details', {}).items():
                doc_details_summary[ext]['files'] += details.get('files', 0)
                doc_details_summary[ext]['lines'] += details.get('lines', 0)
                doc_details_summary[ext]['size'] += details.get('size', 0)

        # 汇总文件大小分布
        size_distribution_summary = {
            'tiny': 0, 'small': 0, 'medium': 0, 'large': 0, 'huge': 0
        }
        for repo in all_repos:
            repo_dist = repo.get('size_distribution', {})
            for key in size_distribution_summary:
                size_distribution_summary[key] += repo_dist.get(key, 0)

        # 汇总复杂度数据
        total_functions = sum(
            repo.get('complexity', {}).get('functions', 0) for repo in all_repos
        )
        total_complexity = sum(
            repo.get('complexity', {}).get('total_complexity', 0) for repo in all_repos
        )
        max_depth = max(
            (repo.get('complexity', {}).get('max_depth', 0) for repo in all_repos),
            default=0
        )
        avg_complexity = total_complexity / total_all_files if total_all_files > 0 else 0
        avg_func_lines = total_all_code_lines / total_functions if total_functions > 0 else 0

        complexity_summary = {
            'functions': total_functions,
            'total_complexity': total_complexity,
            'avg_complexity': round(avg_complexity, 2),
            'max_depth': max_depth,
            'avg_func_lines': round(avg_func_lines, 1)
        }

        language_summary: Dict[str, Dict[str, int]] = defaultdict(
            lambda: {
                'repos': 0, 'files': 0, 'lines': 0,
                'code_lines': 0, 'comment_lines': 0,
                'blank_lines': 0, 'size': 0, 'doc_files': 0,
                'doc_lines': 0, 'doc_size': 0
            }
        )

        for repo in all_repos:
            lang = repo['language']
            language_summary[lang]['repos'] += 1
            language_summary[lang]['files'] += repo['files']
            language_summary[lang]['lines'] += repo['lines']
            language_summary[lang]['code_lines'] += repo.get('code_lines', repo['lines'])
            language_summary[lang]['comment_lines'] += repo.get('comment_lines', 0)
            language_summary[lang]['blank_lines'] += repo.get('blank_lines', 0)
            language_summary[lang]['size'] += repo.get('size', 0)
            language_summary[lang]['doc_files'] += repo.get('doc_files', 0)
            language_summary[lang]['doc_lines'] += repo.get('doc_lines', 0)
            language_summary[lang]['doc_size'] += repo.get('doc_size', 0)

        return {
            'total_repos': len(all_repos),
            'total_files': total_all_files,
            'total_lines': total_all_lines,
            'total_code_lines': total_all_code_lines,
            'total_comment_lines': total_all_comment_lines,
            'total_blank_lines': total_all_blank_lines,
            'total_size': total_all_size,
            'total_doc_files': total_all_doc_files,
            'total_doc_lines': total_all_doc_lines,
            'total_doc_size': total_all_doc_size,
            'total_all_size': total_all_total_size,
            'by_language': dict(language_summary),
            'code_details': dict(code_details_summary),
            'doc_details': dict(doc_details_summary),
            'size_distribution': size_distribution_summary,
            'complexity': complexity_summary
        }

    def run(self) -> None:
        """运行统计"""
        start_time = time.time()

        # 分析所有仓库
        all_repos = self.analyze_all_repos()

        # 生成摘要
        summary = self.generate_summary(all_repos)

        # 根据输出格式输出结果
        output_kwargs = {
            'output_file': getattr(self.args, 'output_file', None),
            'current_dir': self.current_dir,
            'summary_only': getattr(self.args, 'summary', False),
            'quiet': getattr(self.args, 'quiet', False),
        }

        if self.args.output == 'json':
            JsonOutput(**output_kwargs).output(all_repos, summary)
        elif self.args.output == 'csv':
            CsvOutput(**output_kwargs).output(all_repos, summary)
        elif self.args.output == 'markdown':
            MarkdownOutput(
                **output_kwargs,
                sort_by=getattr(self.args, 'sort', 'lines'),
                top=getattr(self.args, 'top', None),
            ).output(all_repos, summary)
        elif self.args.output == 'html':
            HtmlOutput(**output_kwargs).output(all_repos, summary)

        # 始终输出到控制台（除非指定了 quiet）
        if not self.args.quiet:
            ConsoleOutput(**output_kwargs).output(all_repos, summary)

        elapsed_time = time.time() - start_time
        if self.args.verbose:
            print(f"\n统计完成，耗时: {elapsed_time:.2f} 秒")
