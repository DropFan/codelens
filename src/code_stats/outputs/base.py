"""输出器基类"""

from abc import ABC, abstractmethod
from typing import Any, Dict, List, Optional


class BaseOutput(ABC):
    """输出器基类"""

    def __init__(
        self,
        output_file: Optional[str] = None,
        current_dir: str = "",
        summary_only: bool = False,
        quiet: bool = False,
    ):
        """
        初始化输出器

        Args:
            output_file: 输出文件路径
            current_dir: 当前工作目录
            summary_only: 是否只显示摘要
            quiet: 安静模式
        """
        self.output_file = output_file
        self.current_dir = current_dir
        self.summary_only = summary_only
        self.quiet = quiet

    @abstractmethod
    def output(
        self,
        all_repos: List[Dict[str, Any]],
        summary: Dict[str, Any]
    ) -> None:
        """输出统计结果

        Args:
            all_repos: 所有仓库的统计数据
            summary: 统计摘要
        """
        pass

    def _print_saved_message(self, filepath: str) -> None:
        """打印保存成功消息"""
        if not self.summary_only and not self.quiet:
            print(f"\n统计结果已保存到: {filepath}")
