"""文件分析器测试"""

from pathlib import Path

import pytest

from code_stats.analyzers.file import FileContentAnalyzer


class TestFileContentAnalyzer:
    """FileContentAnalyzer 类测试"""

    def test_analyze_python_file(self, sample_python_file: Path):
        """分析 Python 文件"""
        analyzer = FileContentAnalyzer()
        result = analyzer.analyze(str(sample_python_file))

        assert result is not None
        assert result.total > 0
        assert result.code > 0
        assert result.comment > 0  # 有文档字符串和注释
        assert result.blank > 0

    def test_analyze_go_file(self, sample_go_file: Path):
        """分析 Go 文件"""
        analyzer = FileContentAnalyzer()
        result = analyzer.analyze(str(sample_go_file))

        assert result is not None
        assert result.total > 0
        assert result.code > 0
        assert result.comment > 0  # 有注释

    def test_analyze_nonexistent_file(self):
        """分析不存在的文件应返回空 FileStats"""
        analyzer = FileContentAnalyzer()
        result = analyzer.analyze("/nonexistent/file.py")
        assert result.total == 0
        assert result.code == 0

    def test_analyze_empty_file(self, temp_dir: Path):
        """分析空文件"""
        empty_file = temp_dir / "empty.py"
        empty_file.write_text("", encoding='utf-8')

        analyzer = FileContentAnalyzer()
        result = analyzer.analyze(str(empty_file))

        # 空文件被解析为 1 行（空字符串 split('\n') 返回 ['']）
        assert result.total == 1
        assert result.blank == 1

    def test_line_count_accuracy(self, temp_dir: Path):
        """测试行数统计准确性"""
        code = '''# Comment line
def foo():
    pass

# Another comment
x = 1
'''
        test_file = temp_dir / "test.py"
        test_file.write_text(code, encoding='utf-8')

        analyzer = FileContentAnalyzer()
        result = analyzer.analyze(str(test_file))

        # 7 行总计（包括末尾换行产生的空行）
        assert result.total == 7
        assert result.comment == 2  # 两行注释
        assert result.blank >= 1     # 至少一行空行
        assert result.code >= 3      # def, pass, x = 1

    def test_min_lines_filter(self, temp_dir: Path):
        """测试最小行数过滤"""
        short_file = temp_dir / "short.py"
        short_file.write_text("x = 1\n", encoding='utf-8')

        analyzer = FileContentAnalyzer(min_lines=10)
        result = analyzer.analyze(str(short_file))

        # 文件行数少于 min_lines，返回空 FileStats
        assert result.total == 0
        assert result.code == 0

    def test_max_lines_filter(self, temp_dir: Path):
        """测试最大行数过滤"""
        long_content = "\n".join([f"x{i} = {i}" for i in range(100)])
        long_file = temp_dir / "long.py"
        long_file.write_text(long_content, encoding='utf-8')

        analyzer = FileContentAnalyzer(max_lines=50)
        result = analyzer.analyze(str(long_file))

        # 文件行数超过 max_lines，返回空 FileStats
        assert result.total == 0
        assert result.code == 0
