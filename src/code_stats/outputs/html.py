"""HTML 输出"""

from datetime import datetime
from pathlib import Path
from typing import Any, Dict, List, Optional

from jinja2 import Environment, FileSystemLoader  # type: ignore[import-not-found]

from code_stats import __version__
from code_stats.outputs.base import BaseOutput
from code_stats.utils.formatters import format_size, get_language_emoji


class HtmlOutput(BaseOutput):
    """HTML 输出器（使用 Jinja2 模板）"""

    def __init__(
        self,
        output_file: Optional[str] = None,
        current_dir: str = "",
        summary_only: bool = False,
        quiet: bool = False,
    ):
        super().__init__(output_file, current_dir, summary_only, quiet)

        # 设置 Jinja2 模板环境
        template_dir = Path(__file__).parent / "templates"
        self.env = Environment(
            loader=FileSystemLoader(str(template_dir)),
            autoescape=True
        )
        # 添加自定义过滤器
        self.env.globals['format_size'] = format_size
        self.env.globals['get_emoji'] = get_language_emoji

    def output(
        self,
        all_repos: List[Dict[str, Any]],
        summary: Dict[str, Any]
    ) -> None:
        """输出 HTML 格式"""
        output_file = self.output_file or 'code_statistics.html'

        # 准备数据
        gen_time = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        code_details = summary.get('code_details', {})
        doc_details = summary.get('doc_details', {})

        # 类型分布（前10个）
        sorted_code_types = sorted(
            code_details.items(),
            key=lambda x: x[1]['lines'],
            reverse=True
        )[:10]
        sorted_doc_types = sorted(
            doc_details.items(),
            key=lambda x: x[1]['lines'],
            reverse=True
        )[:10]

        sorted_languages = sorted(
            summary['by_language'].items(),
            key=lambda x: x[1]['lines'],
            reverse=True
        )
        sorted_repos = sorted(all_repos, key=lambda x: x['lines'], reverse=True)

        total_lines = summary['total_lines']

        # 计算比率
        code_rate = (
            (summary.get('total_code_lines', 0) / total_lines * 100)
            if total_lines > 0 else 0
        )
        comment_rate = (
            (summary.get('total_comment_lines', 0) / total_lines * 100)
            if total_lines > 0 else 0
        )
        blank_rate = (
            (summary.get('total_blank_lines', 0) / total_lines * 100)
            if total_lines > 0 else 0
        )

        # 文件大小分布数据
        size_dist = summary.get('size_distribution', {})
        size_labels = ['< 1KB', '1-10KB', '10-100KB', '100KB-1MB', '> 1MB']
        size_data = [
            size_dist.get('tiny', 0),
            size_dist.get('small', 0),
            size_dist.get('medium', 0),
            size_dist.get('large', 0),
            size_dist.get('huge', 0)
        ]

        # 复杂度数据
        complexity = summary.get('complexity', {
            'functions': 0,
            'avg_complexity': 0,
            'max_depth': 0,
            'avg_func_lines': 0
        })

        # 图表数据
        lang_labels = [lang for lang, _ in sorted_languages]
        lang_data = [data['lines'] for _, data in sorted_languages]
        repo_labels = [repo['name'] for repo in sorted_repos]
        repo_data = [repo['lines'] for repo in sorted_repos]

        # Git 信息
        git_repos = [r for r in all_repos if r.get('git')]

        # 渲染模板
        template = self.env.get_template('report.html')
        html = template.render(
            gen_time=gen_time,
            current_dir=self.current_dir,
            summary=summary,
            total_size_formatted=format_size(summary.get('total_all_size', 0)),
            code_rate=code_rate,
            comment_rate=comment_rate,
            blank_rate=blank_rate,
            code_types=sorted_code_types,
            doc_types=sorted_doc_types,
            complexity=complexity,
            size_labels=size_labels,
            size_data=size_data,
            lang_labels=lang_labels,
            lang_data=lang_data,
            repo_labels=repo_labels,
            repo_data=repo_data,
            languages=sorted_languages,
            repos=sorted_repos,
            git_repos=git_repos,
            version=__version__,
        )

        with open(output_file, 'w', encoding='utf-8') as f:
            f.write(html)

        self._print_saved_message(output_file)
