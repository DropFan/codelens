"""命令行接口"""

import argparse
import sys
from typing import List, Optional

from code_stats import __version__
from code_stats.config import (
    load_config_file,
    merge_args_with_config,
    show_supported_languages,
)
from code_stats.core import CodeStatistics


def create_parser() -> argparse.ArgumentParser:
    """创建命令行参数解析器

    Returns:
        ArgumentParser: 配置好的参数解析器
    """
    parser = argparse.ArgumentParser(
        description='代码统计工具 - 统计仓库代码行数',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
示例:
  %(prog)s                      # 基本统计
  %(prog)s --all                # 统计所有文件
  %(prog)s --lang python,go     # 只统计Python和Go代码
  %(prog)s --excludes "*test*"  # 排除测试文件
  %(prog)s --output markdown    # 输出Markdown格式
  %(prog)s --repo api,gateway   # 只统计特定仓库
  %(prog)s --top 20             # 只显示前20个结果
  %(prog)s --dirs api admin     # 只统计指定目录
  %(prog)s --exclude-files ".*_test\\.py$,.*\\.bak$"  # 排除测试文件和备份文件
  %(prog)s --exclude-dirs ".*/test/.*,.*/docs/.*"   # 排除test和docs目录
  %(prog)s --help-lang          # 显示支持的编程语言列表
  %(prog)s --config my.yaml     # 使用指定配置文件
  %(prog)s --depth 3            # 限制扫描深度为3层
  %(prog)s --git-info           # 显示Git仓库信息
  %(prog)s --include-files ".*\\.py$"  # 只包含Python文件
  %(prog)s --no-gitignore       # 禁用 .gitignore 规则过滤

配置文件示例 (.code_stats.yaml):
  excludes: "*test*,*mock*"
  lang: python,go,javascript
  output: html
  parallel: true
  depth: 5
  git_info: true
  no_gitignore: false  # 默认启用 .gitignore 过滤
        """
    )

    # 版本号
    parser.add_argument(
        '--version', '-V',
        action='version',
        version=f'%(prog)s {__version__}'
    )

    # 显示支持的语言
    parser.add_argument(
        '--help-lang',
        action='store_true',
        help='显示支持的编程语言列表'
    )

    # 基本参数
    parser.add_argument(
        '--all', '-a',
        action='store_true',
        help='统计所有文件（包括依赖和文档）'
    )
    parser.add_argument(
        '--excludes',
        type=str,
        help='额外的排除模式，用逗号分隔（支持通配符）'
    )
    parser.add_argument(
        '--exclude-files',
        type=str,
        help='排除文件的正则表达式模式，用逗号分隔（如: .*_test\\.py$,.*\\.bak$）'
    )
    parser.add_argument(
        '--exclude-dirs',
        type=str,
        help='排除目录的正则表达式模式，用逗号分隔（如: .*/test/.*,.*/backup/.*）'
    )
    parser.add_argument(
        '--lang',
        type=str,
        help='指定统计的编程语言，用逗号分隔（如: python,go,javascript）'
    )
    parser.add_argument(
        '--dirs', '-d',
        type=str,
        action='append',
        help='指定要统计的目录（可多次使用，如: -d api -d admin）'
    )

    # 输出格式
    parser.add_argument(
        '--output', '-o',
        choices=['json', 'csv', 'markdown', 'html'],
        help='输出格式（默认输出到控制台）'
    )
    parser.add_argument(
        '--output-file', '-O',
        type=str,
        help='输出文件名（默认根据格式自动命名）'
    )

    # 过滤选项
    parser.add_argument(
        '--repo',
        type=str,
        help='指定特定仓库，用逗号分隔'
    )
    parser.add_argument(
        '--min-lines',
        type=int,
        help='只统计行数大于指定值的文件'
    )
    parser.add_argument(
        '--max-lines',
        type=int,
        help='只统计行数小于指定值的文件'
    )

    # 显示选项
    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='显示详细信息'
    )
    parser.add_argument(
        '--summary', '-s',
        action='store_true',
        help='只显示摘要信息'
    )
    parser.add_argument(
        '--quiet', '-q',
        action='store_true',
        help='安静模式（不输出到控制台）'
    )
    parser.add_argument(
        '--sort',
        choices=['lines', 'files', 'name'],
        default='lines',
        help='排序方式（默认按行数）'
    )
    parser.add_argument(
        '--top',
        type=int,
        help='只显示前N个结果'
    )

    # 性能选项
    parser.add_argument(
        '--parallel', '-p',
        action='store_true',
        help='使用多线程并行处理'
    )

    # 配置文件选项
    parser.add_argument(
        '--config', '-c',
        type=str,
        help='指定配置文件路径（支持 .yaml/.yml/.json）'
    )
    parser.add_argument(
        '--no-config',
        action='store_true',
        help='不加载配置文件'
    )

    # 高级选项
    parser.add_argument(
        '--include-files',
        type=str,
        help='包含文件的正则表达式模式，用逗号分隔'
    )
    parser.add_argument(
        '--depth',
        type=int,
        help='目录扫描深度限制（0 表示无限制）'
    )
    parser.add_argument(
        '--git-info',
        action='store_true',
        help='显示 Git 仓库信息（最后提交时间、作者等）'
    )
    parser.add_argument(
        '--no-smart-exclude',
        action='store_true',
        help='禁用智能排除，使用完整的排除目录列表（默认启用智能排除）'
    )
    parser.add_argument(
        '--no-gitignore',
        action='store_true',
        help='禁用 .gitignore 规则过滤（默认启用）'
    )

    # Shell 补全
    parser.add_argument(
        '--install-completion',
        action='store_true',
        help='安装 Shell 自动补全脚本'
    )

    # 尝试加载补全器（argcomplete 可选）
    try:
        from code_stats.completion import setup_completers
        setup_completers(parser)
    except ImportError:
        pass

    return parser


def main(argv: Optional[List[str]] = None) -> None:
    """主函数

    Args:
        argv: 命令行参数列表（用于测试），默认为 None 使用 sys.argv
    """
    parser = create_parser()

    # 启用 argcomplete 自动补全（必须在 parse_args 之前）
    try:
        import argcomplete
        argcomplete.autocomplete(parser)
    except ImportError:
        pass

    args = parser.parse_args(argv)

    # 处理 --install-completion 参数
    if args.install_completion:
        try:
            from code_stats.completion import install_completion
            success, message = install_completion()
            print(message)
            sys.exit(0 if success else 1)
        except ImportError:
            print("错误: 需要安装 argcomplete (pip install code-stats[completion])",
                  file=sys.stderr)
            sys.exit(1)

    # 如果请求显示支持的语言列表
    if args.help_lang:
        show_supported_languages()
        return

    # 加载配置文件（除非指定 --no-config）
    if not args.no_config:
        config = load_config_file(args.config)
        args = merge_args_with_config(args, config)

    # 创建统计实例并运行
    stats = CodeStatistics(args)
    stats.run()


if __name__ == "__main__":
    main()
