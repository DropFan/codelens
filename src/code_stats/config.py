"""配置加载和管理"""

import json
import os
from argparse import Namespace
from typing import Any, Dict, List, Optional

from code_stats.constants import LANGUAGE_MAP

# 尝试导入 yaml（可选依赖）
try:
    import yaml  # type: ignore[import-not-found]
    HAS_YAML = True
except ImportError:
    yaml = None  # type: ignore[assignment]
    HAS_YAML = False


def load_config_file(config_path: Optional[str] = None) -> Dict[str, Any]:
    """加载配置文件

    配置文件搜索顺序:
    1. 命令行指定的路径
    2. 当前目录的 .code_stats.yaml 或 .code_stats.yml
    3. 当前目录的 .code_stats.json
    4. 用户目录的 ~/.code_stats.yaml

    Args:
        config_path: 配置文件路径（可选）

    Returns:
        dict: 配置字典，如果没有找到配置文件则返回空字典
    """
    config: Dict[str, Any] = {}

    # 确定配置文件路径
    search_paths: List[str] = []
    if config_path:
        search_paths.append(config_path)
    else:
        cwd = os.getcwd()
        search_paths.extend([
            os.path.join(cwd, '.code_stats.yaml'),
            os.path.join(cwd, '.code_stats.yml'),
            os.path.join(cwd, '.code_stats.json'),
            os.path.expanduser('~/.code_stats.yaml'),
            os.path.expanduser('~/.code_stats.yml'),
        ])

    config_file = None
    for path in search_paths:
        if os.path.exists(path):
            config_file = path
            break

    if not config_file:
        return config

    try:
        with open(config_file, 'r', encoding='utf-8') as f:
            if config_file.endswith('.json'):
                config = json.load(f)
            elif HAS_YAML and (config_file.endswith('.yaml') or config_file.endswith('.yml')):
                config = yaml.safe_load(f) or {}  # type: ignore
            else:
                # 如果没有 yaml 模块，尝试用 JSON 解析
                print(f"警告：未安装 PyYAML，无法解析 {config_file}")
                print("请运行: pip install pyyaml")
                return config

        print(f"已加载配置文件: {config_file}")
    except Exception as e:
        print(f"警告：读取配置文件失败 {config_file}: {e}")

    return config


def merge_args_with_config(args: Namespace, config: Dict[str, Any]) -> Namespace:
    """将配置文件中的设置合并到参数中（命令行参数优先）

    Args:
        args: 命令行参数
        config: 配置文件字典

    Returns:
        Namespace: 合并后的参数
    """
    # 映射配置文件键到参数属性
    config_mapping = {
        'excludes': 'excludes',
        'exclude_files': 'exclude_files',
        'exclude_dirs': 'exclude_dirs',
        'include_files': 'include_files',
        'lang': 'lang',
        'output': 'output',
        'output_file': 'output_file',
        'min_lines': 'min_lines',
        'max_lines': 'max_lines',
        'sort': 'sort',
        'top': 'top',
        'parallel': 'parallel',
        'verbose': 'verbose',
        'summary': 'summary',
        'quiet': 'quiet',
        'all': 'all',
        'depth': 'depth',
        'git_info': 'git_info',
        'no_gitignore': 'no_gitignore',
    }

    for config_key, arg_attr in config_mapping.items():
        if config_key in config:
            # 只在命令行没有指定时使用配置文件的值
            current_value = getattr(args, arg_attr, None)
            if current_value is None or current_value is False or current_value == []:
                setattr(args, arg_attr, config[config_key])

    # 处理特殊的 dirs 参数（列表类型）
    if 'dirs' in config and not args.dirs:
        args.dirs = config['dirs']

    # 处理特殊的 repo 参数
    if 'repo' in config and not args.repo:
        args.repo = config['repo']

    return args


def show_supported_languages() -> None:
    """显示支持的编程语言列表"""
    # 收集所有唯一的语言
    languages = sorted(set(LANGUAGE_MAP.values()))

    print("支持的编程语言列表:")
    print("=" * 60)

    # 按语言分组显示扩展名
    lang_to_exts: Dict[str, List[str]] = {}
    for ext, lang in LANGUAGE_MAP.items():
        if lang not in lang_to_exts:
            lang_to_exts[lang] = []
        lang_to_exts[lang].append(ext)

    for lang in languages:
        exts = sorted(lang_to_exts.get(lang, []))
        exts_str = ', '.join(exts[:8])
        if len(lang_to_exts.get(lang, [])) > 8:
            exts_str += f', ... (+{len(lang_to_exts[lang]) - 8})'
        print(f"  {lang:18} : {exts_str}")

    print("\n" + "=" * 60)
    print(f"共支持 {len(languages)} 种编程语言")
    print("\n使用方法: --lang python,go,javascript")
