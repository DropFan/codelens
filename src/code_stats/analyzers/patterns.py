"""语言特定的分析模式"""

from typing import Dict, List, Optional, Tuple

# 单行注释符号
SINGLE_LINE_COMMENTS: Dict[str, str] = {
    'python': '#', 'ruby': '#', 'perl': '#', 'shell': '#', 'yaml': '#',
    'javascript': '//', 'typescript': '//', 'java': '//', 'cpp': '//',
    'c': '//', 'csharp': '//', 'go': '//', 'rust': '//', 'swift': '//',
    'kotlin': '//', 'scala': '//', 'dart': '//', 'php': '//',
    'sql': '--', 'lua': '--', 'haskell': '--', 'elm': '--',
    'assembly': ';', 'lisp': ';', 'clojure': ';', 'scheme': ';',
    'fortran': '!', 'vbnet': "'", 'matlab': '%', 'latex': '%',
    'erlang': '%', 'prolog': '%'
}

# 多行注释符号 (开始, 结束)
MULTI_LINE_COMMENTS: Dict[str, Tuple[str, str]] = {
    'c': ('/*', '*/'), 'cpp': ('/*', '*/'), 'java': ('/*', '*/'),
    'javascript': ('/*', '*/'), 'typescript': ('/*', '*/'),
    'css': ('/*', '*/'), 'php': ('/*', '*/'), 'go': ('/*', '*/'),
    'rust': ('/*', '*/'), 'swift': ('/*', '*/'), 'kotlin': ('/*', '*/'),
    'scala': ('/*', '*/'), 'dart': ('/*', '*/'), 'sql': ('/*', '*/'),
    'html': ('<!--', '-->'), 'xml': ('<!--', '-->'),
    'python': ('"""', '"""'), 'ruby': ('=begin', '=end'),
    'lua': ('--[[', ']]'), 'haskell': ('{-', '-}')
}

# 复杂度关键字（按语言）
COMPLEXITY_KEYWORDS: Dict[str, List[str]] = {
    'python': ['if ', 'elif ', 'for ', 'while ', 'except ', 'with ', 'and ', 'or ', 'case '],
    'javascript': ['if ', 'else if ', 'for ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
    'typescript': ['if ', 'else if ', 'for ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
    'go': ['if ', 'else if ', 'for ', 'switch ', 'case ', 'select ', '&&', '||'],
    'java': ['if ', 'else if ', 'for ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
    'cpp': ['if ', 'else if ', 'for ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
    'c': ['if ', 'else if ', 'for ', 'while ', 'case ', '&&', '||', '\\?'],
    'rust': ['if ', 'else if ', 'for ', 'while ', 'match ', '=>', '&&', '||'],
    'php': ['if ', 'elseif ', 'for ', 'foreach ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
}

# 函数定义正则模式
FUNCTION_PATTERNS: Dict[str, str] = {
    'python': r'^\s*def\s+\w+|^\s*async\s+def\s+\w+|^\s*class\s+\w+',
    'javascript': r'function\s+\w+|^\s*\w+\s*[=:]\s*(?:async\s*)?\(|^\s*(?:async\s+)?(?:function|\w+)\s*\(',
    'typescript': r'function\s+\w+|^\s*\w+\s*[=:]\s*(?:async\s*)?\(|^\s*(?:async\s+)?(?:function|\w+)\s*\(',
    'go': r'func\s+(?:\(\w+\s+\*?\w+\)\s*)?\w+',
    'java': r'(?:public|private|protected|static|\s)+[\w<>\[\]]+\s+\w+\s*\([^)]*\)\s*(?:throws\s+[\w,\s]+)?\s*\{',
    'cpp': r'(?:\w+\s+)+\w+::\w+\s*\(|(?:\w+\s+)+\w+\s*\([^)]*\)\s*\{',
    'c': r'(?:\w+\s+)+\w+\s*\([^)]*\)\s*\{',
    'rust': r'fn\s+\w+|impl\s+\w+',
    'php': r'function\s+\w+|public\s+function|private\s+function|protected\s+function',
}


def get_single_line_comment(lang: str) -> str:
    """获取语言的单行注释符号"""
    return SINGLE_LINE_COMMENTS.get(lang, '#')


def get_multi_line_comment(lang: str) -> Optional[Tuple[str, str]]:
    """获取语言的多行注释符号"""
    return MULTI_LINE_COMMENTS.get(lang)


def get_complexity_keywords(lang: str) -> List[str]:
    """获取语言的复杂度关键字"""
    return COMPLEXITY_KEYWORDS.get(lang, [])


def get_function_pattern(lang: str) -> Optional[str]:
    """获取语言的函数定义正则模式"""
    return FUNCTION_PATTERNS.get(lang)
