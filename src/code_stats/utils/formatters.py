"""格式化工具函数"""

from typing import Dict


def format_size(size_in_bytes: int) -> str:
    """格式化文件大小为人类可读的字符串

    Args:
        size_in_bytes: 字节大小

    Returns:
        格式化后的大小字符串，如 "1.5 MB"
    """
    if size_in_bytes < 1024:
        return f"{size_in_bytes} B"
    elif size_in_bytes < 1024 * 1024:
        return f"{size_in_bytes / 1024:.1f} KB"
    elif size_in_bytes < 1024 * 1024 * 1024:
        return f"{size_in_bytes / (1024 * 1024):.1f} MB"
    else:
        return f"{size_in_bytes / (1024 * 1024 * 1024):.1f} GB"


# 语言对应的 emoji 图标映射
LANGUAGE_EMOJI_MAP: Dict[str, str] = {
    'python': '🐍',
    'javascript': '📜',
    'typescript': '📘',
    'java': '☕',
    'go': '🐹',
    'rust': '🦀',
    'cpp': '⚙️',
    'c': '🔧',
    'csharp': '🔷',
    'ruby': '💎',
    'php': '🐘',
    'swift': '🦉',
    'kotlin': '🟣',
    'scala': '🔴',
    'r': '📊',
    'shell': '🐚',
    'html': '🌐',
    'css': '🎨',
    'sql': '🗄️',
    'dart': '🎯',
    'vue': '💚',
    'react': '⚛️',
    'docker': '🐳',
    'yaml': '📝',
    'json': '📋',
    'xml': '📄',
    'markdown': '📑',
    'terraform': '🏗️',
    'kubernetes': '☸️',
    'haskell': '🎓',
    'elixir': '💧',
    'erlang': '📡',
    'julia': '🟢',
    'matlab': '🔬',
    'perl': '🐪',
    'lua': '🌙',
    'nim': '👑',
    'zig': '⚡',
    'assembly': '🔩',
    'fortran': '🏛️',
    'cobol': '🏦',
    'pascal': '🔺',
    'lisp': '🎭',
    'clojure': '☯️',
    'ocaml': '🐫',
    'fsharp': '📐',
    'vbnet': '🔵',
    'powershell': '💠',
    'batch': '📦',
    'make': '🔨',
    'cmake': '🛠️',
    'gradle': '🐘',
    'maven': '🏗️',
    'unknown': '❓',
    'other': '📌'
}


def get_language_emoji(language: str) -> str:
    """获取语言对应的 emoji 图标

    Args:
        language: 编程语言名称

    Returns:
        对应的 emoji 字符
    """
    return LANGUAGE_EMOJI_MAP.get(language.lower(), '📄')
