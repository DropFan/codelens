"""文件过滤器测试"""

import pytest

from code_stats.filters.file_filter import is_binary_file, is_code_file


class TestIsCodeFile:
    """is_code_file 函数测试"""

    def test_python_file(self):
        """Python 文件应该被识别为代码文件"""
        assert is_code_file("main.py") is True
        assert is_code_file("test_main.py") is True
        assert is_code_file("src/utils.py") is True

    def test_go_file(self):
        """Go 文件应该被识别为代码文件"""
        assert is_code_file("main.go") is True
        assert is_code_file("pkg/server/handler.go") is True

    def test_javascript_file(self):
        """JavaScript 文件应该被识别为代码文件"""
        assert is_code_file("index.js") is True
        assert is_code_file("app.ts") is True
        assert is_code_file("component.tsx") is True
        assert is_code_file("style.jsx") is True

    def test_config_file(self):
        """配置文件在非 include_all 模式下不应该被识别为代码文件"""
        assert is_code_file("package.json", include_all=False) is False
        assert is_code_file("config.yaml", include_all=False) is False

    def test_config_file_include_all(self):
        """配置文件在 include_all 模式下应该被识别"""
        assert is_code_file("package.json", include_all=True) is True
        assert is_code_file("config.yaml", include_all=True) is True

    def test_doc_file(self):
        """文档文件不应该被识别为代码文件"""
        assert is_code_file("README.md", include_all=False) is False
        assert is_code_file("docs/guide.rst", include_all=False) is False

    def test_binary_file(self):
        """二进制文件不应该被识别为代码文件"""
        assert is_code_file("image.png") is False
        assert is_code_file("data.bin") is False

    def test_hidden_file(self):
        """隐藏文件不应该被识别为代码文件"""
        assert is_code_file(".gitignore") is False
        assert is_code_file(".env") is False

    def test_language_filter(self):
        """语言过滤应该正确工作"""
        target_langs = {"python", "go"}
        assert is_code_file("main.py", target_languages=target_langs) is True
        assert is_code_file("main.go", target_languages=target_langs) is True
        assert is_code_file("main.js", target_languages=target_langs) is False


class TestIsBinaryFile:
    """is_binary_file 函数测试"""

    def test_binary_content(self, temp_dir):
        """包含二进制内容的文件应被识别为二进制文件"""
        binary_file = temp_dir / "binary.bin"
        binary_file.write_bytes(b'\x00\x01\x02\x03\xff\xfe')
        assert is_binary_file(str(binary_file)) is True

    def test_text_content(self, temp_dir):
        """纯文本内容不应被识别为二进制文件"""
        text_file = temp_dir / "text.py"
        text_file.write_text("print('hello')\n", encoding='utf-8')
        assert is_binary_file(str(text_file)) is False

    def test_nonexistent_file(self):
        """不存在的文件应被视为二进制（无法读取）"""
        assert is_binary_file("/nonexistent/file.txt") is True

    def test_empty_file(self, temp_dir):
        """空文件不应被识别为二进制文件"""
        empty_file = temp_dir / "empty.txt"
        empty_file.write_bytes(b'')
        assert is_binary_file(str(empty_file)) is False
