"""分析器基础类型定义"""

from dataclasses import dataclass, field
from typing import Dict


@dataclass
class FileStats:
    """文件统计数据"""
    total: int = 0
    code: int = 0
    comment: int = 0
    blank: int = 0
    size: int = 0
    functions: int = 0
    complexity: int = 0
    max_depth: int = 0

    def to_dict(self) -> Dict[str, int]:
        """转换为字典"""
        return {
            'total': self.total,
            'code': self.code,
            'comment': self.comment,
            'blank': self.blank,
            'size': self.size,
            'functions': self.functions,
            'complexity': self.complexity,
            'max_depth': self.max_depth,
        }

    @classmethod
    def empty(cls) -> 'FileStats':
        """返回空的统计数据"""
        return cls()


@dataclass
class ExtensionStats:
    """按扩展名的统计数据"""
    files: int = 0
    lines: int = 0
    code_lines: int = 0
    comment_lines: int = 0
    blank_lines: int = 0
    size: int = 0


@dataclass
class RepositoryStats:
    """仓库统计数据"""
    total_files: int = 0
    total_lines: int = 0
    total_code_lines: int = 0
    total_comment_lines: int = 0
    total_blank_lines: int = 0
    total_size: int = 0
    doc_files: int = 0
    doc_lines: int = 0
    doc_size: int = 0
    total_all_size: int = 0
    by_extension: Dict[str, dict] = field(default_factory=dict)
    doc_details: Dict[str, dict] = field(default_factory=dict)
    file_details: list = field(default_factory=list)
    size_distribution: Dict[str, int] = field(default_factory=lambda: {
        'tiny': 0, 'small': 0, 'medium': 0, 'large': 0, 'huge': 0
    })
    complexity: Dict[str, float] = field(default_factory=lambda: {
        'functions': 0,
        'total_complexity': 0,
        'avg_complexity': 0.0,
        'max_depth': 0,
        'avg_func_lines': 0.0
    })

    def to_dict(self) -> dict:
        """转换为字典"""
        return {
            'total_files': self.total_files,
            'total_lines': self.total_lines,
            'total_code_lines': self.total_code_lines,
            'total_comment_lines': self.total_comment_lines,
            'total_blank_lines': self.total_blank_lines,
            'total_size': self.total_size,
            'doc_files': self.doc_files,
            'doc_lines': self.doc_lines,
            'doc_size': self.doc_size,
            'total_all_size': self.total_all_size,
            'by_extension': self.by_extension,
            'doc_details': self.doc_details,
            'file_details': self.file_details,
            'size_distribution': self.size_distribution,
            'complexity': self.complexity,
        }
