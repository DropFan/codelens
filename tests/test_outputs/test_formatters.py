"""格式化工具测试"""

import pytest

from code_stats.utils.formatters import format_size, get_language_emoji


class TestFormatSize:
    """format_size 函数测试"""

    def test_bytes(self):
        """测试字节级别"""
        assert format_size(0) == "0 B"
        assert format_size(100) == "100 B"
        assert format_size(1023) == "1023 B"

    def test_kilobytes(self):
        """测试 KB 级别"""
        assert format_size(1024) == "1.0 KB"
        assert format_size(1536) == "1.5 KB"
        assert format_size(10240) == "10.0 KB"

    def test_megabytes(self):
        """测试 MB 级别"""
        assert format_size(1024 * 1024) == "1.0 MB"
        assert format_size(5 * 1024 * 1024) == "5.0 MB"

    def test_gigabytes(self):
        """测试 GB 级别"""
        assert format_size(1024 * 1024 * 1024) == "1.0 GB"
        assert format_size(2 * 1024 * 1024 * 1024) == "2.0 GB"


class TestGetLanguageEmoji:
    """get_language_emoji 函数测试"""

    def test_known_languages(self):
        """测试已知语言的 emoji"""
        assert get_language_emoji("python") == "🐍"
        assert get_language_emoji("go") == "🐹"
        assert get_language_emoji("javascript") == "📜"
        assert get_language_emoji("rust") == "🦀"

    def test_unknown_language(self):
        """测试未知语言返回默认 emoji"""
        assert get_language_emoji("unknown_lang") == "📄"
        assert get_language_emoji("") == "📄"

    def test_case_sensitivity(self):
        """测试大小写处理"""
        # 函数应该接受小写语言名
        assert get_language_emoji("python") == "🐍"
