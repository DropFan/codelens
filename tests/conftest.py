"""pytest 配置和共享 fixtures"""

import tempfile
from pathlib import Path
from typing import Generator

import pytest


@pytest.fixture
def temp_dir() -> Generator[Path, None, None]:
    """创建临时目录"""
    with tempfile.TemporaryDirectory() as tmpdir:
        yield Path(tmpdir)


@pytest.fixture
def sample_python_file(temp_dir: Path) -> Path:
    """创建示例 Python 文件"""
    code = '''#!/usr/bin/env python3
"""示例模块"""

def hello(name: str) -> str:
    """打招呼函数"""
    # 返回问候语
    return f"Hello, {name}!"


def add(a: int, b: int) -> int:
    """加法函数"""
    return a + b


class Calculator:
    """计算器类"""

    def __init__(self):
        self.result = 0

    def add(self, value: int) -> int:
        """加法"""
        self.result += value
        return self.result

    def reset(self):
        """重置"""
        self.result = 0
'''
    file_path = temp_dir / "sample.py"
    file_path.write_text(code, encoding='utf-8')
    return file_path


@pytest.fixture
def sample_go_file(temp_dir: Path) -> Path:
    """创建示例 Go 文件"""
    code = '''package main

import "fmt"

// Hello returns a greeting
func Hello(name string) string {
    return fmt.Sprintf("Hello, %s!", name)
}

func main() {
    fmt.Println(Hello("World"))
}
'''
    file_path = temp_dir / "main.go"
    file_path.write_text(code, encoding='utf-8')
    return file_path


@pytest.fixture
def sample_repo(temp_dir: Path) -> Path:
    """创建示例仓库结构"""
    # 创建 .git 目录（模拟 git 仓库）
    git_dir = temp_dir / ".git"
    git_dir.mkdir()

    # 创建 Python 文件
    src_dir = temp_dir / "src"
    src_dir.mkdir()

    (src_dir / "main.py").write_text('''
def main():
    print("Hello, World!")

if __name__ == "__main__":
    main()
''', encoding='utf-8')

    (src_dir / "utils.py").write_text('''
def format_size(size: int) -> str:
    """Format file size"""
    if size < 1024:
        return f"{size} B"
    return f"{size / 1024:.1f} KB"
''', encoding='utf-8')

    # 创建配置文件
    (temp_dir / "pyproject.toml").write_text('''
[project]
name = "sample"
version = "0.1.0"
''', encoding='utf-8')

    # 创建 .gitignore
    (temp_dir / ".gitignore").write_text('''
__pycache__/
*.pyc
.venv/
''', encoding='utf-8')

    return temp_dir


@pytest.fixture
def multi_lang_repo(temp_dir: Path) -> Path:
    """创建多语言仓库"""
    git_dir = temp_dir / ".git"
    git_dir.mkdir()

    # Python
    py_dir = temp_dir / "python"
    py_dir.mkdir()
    (py_dir / "app.py").write_text('print("Hello")\n', encoding='utf-8')

    # Go
    go_dir = temp_dir / "go"
    go_dir.mkdir()
    (go_dir / "main.go").write_text('package main\n\nfunc main() {}\n', encoding='utf-8')

    # JavaScript
    js_dir = temp_dir / "js"
    js_dir.mkdir()
    (js_dir / "index.js").write_text('console.log("Hello");\n', encoding='utf-8')

    return temp_dir
