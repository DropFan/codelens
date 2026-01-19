#!/usr/bin/env python3
"""代码统计脚本 - 向后兼容入口

支持以下运行方式：
  python code_statistics.py          # 直接运行（无需安装）
  code-stats                         # 安装后的命令
  python -m code_stats               # 模块运行

详细用法请参考: python code_statistics.py --help
"""
import sys
from pathlib import Path

# 添加 src 目录到路径（支持未安装时直接运行）
src_path = Path(__file__).parent / "src"
if src_path.exists():
    sys.path.insert(0, str(src_path))

from code_stats import __version__
from code_stats.cli import main

if __name__ == "__main__":
    main()
