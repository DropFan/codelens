"""Shell 自动补全支持

提供 bash/zsh/fish 的命令行自动补全功能。
需要安装 argcomplete: pip install argcomplete
"""

import os
from typing import Iterator, Any, Optional, Tuple


def LanguageCompleter(prefix: str, **kwargs: Any) -> Iterator[str]:
    """补全 --lang 参数：支持的编程语言列表"""
    from code_stats.constants.languages import LANGUAGE_MAP

    # 获取所有语言名（去重）
    languages = sorted(set(LANGUAGE_MAP.values()))
    prefix_lower = prefix.lower()

    # 支持逗号分隔的多值输入，只补全最后一个
    if ',' in prefix:
        base = prefix.rsplit(',', 1)[0] + ','
        partial = prefix.rsplit(',', 1)[1].lower()
        for lang in languages:
            if lang.lower().startswith(partial):
                yield base + lang
    else:
        for lang in languages:
            if lang.lower().startswith(prefix_lower):
                yield lang


def DirectoryCompleter(prefix: str, **kwargs: Any) -> Iterator[str]:
    """补全 --dirs 参数：当前目录下的子目录"""
    base_dir = os.path.dirname(prefix) or '.'
    partial = os.path.basename(prefix)

    try:
        for name in os.listdir(base_dir):
            full_path = os.path.join(base_dir, name)
            if os.path.isdir(full_path) and name.startswith(partial):
                # 返回带尾部斜杠的目录路径
                if base_dir == '.':
                    yield name + '/'
                else:
                    yield full_path + '/'
    except OSError:
        pass


def ConfigFileCompleter(prefix: str, **kwargs: Any) -> Iterator[str]:
    """补全 --config 参数：.yaml/.yml/.json 配置文件"""
    try:
        from argcomplete.completers import FilesCompleter
        completer = FilesCompleter(['*.yaml', '*.yml', '*.json', '.code_stats.*'])
        yield from completer(prefix, **kwargs)
    except ImportError:
        # 回退到简单实现
        base_dir = os.path.dirname(prefix) or '.'
        partial = os.path.basename(prefix)
        extensions = ('.yaml', '.yml', '.json')

        try:
            for name in os.listdir(base_dir):
                if name.startswith(partial):
                    if name.endswith(extensions) or name.startswith('.code_stats'):
                        if base_dir == '.':
                            yield name
                        else:
                            yield os.path.join(base_dir, name)
        except OSError:
            pass


def RepositoryCompleter(prefix: str, **kwargs: Any) -> Iterator[str]:
    """补全 --repo 参数：当前目录下的 Git 仓库"""
    # 支持逗号分隔的多值输入
    if ',' in prefix:
        base = prefix.rsplit(',', 1)[0] + ','
        partial = prefix.rsplit(',', 1)[1]
    else:
        base = ''
        partial = prefix

    try:
        for name in os.listdir('.'):
            if os.path.isdir(name) and os.path.isdir(os.path.join(name, '.git')):
                if name.startswith(partial):
                    yield base + name
    except OSError:
        pass


def setup_completers(parser) -> None:
    """为 parser 中的参数绑定补全器

    Args:
        parser: argparse.ArgumentParser 实例
    """
    completer_map = {
        'lang': LanguageCompleter,
        'dirs': DirectoryCompleter,
        'config': ConfigFileCompleter,
        'repo': RepositoryCompleter,
    }

    for action in parser._actions:
        if action.dest in completer_map:
            action.completer = completer_map[action.dest]


def detect_shell() -> Optional[str]:
    """检测当前使用的 Shell 类型

    Returns:
        Shell 名称 (bash/zsh/fish) 或 None
    """
    shell_path = os.environ.get('SHELL', '')
    shell_name = os.path.basename(shell_path)

    if shell_name in ('bash', 'zsh', 'fish'):
        return shell_name
    return None


def get_completion_install_path(shell: str) -> Tuple[str, str]:
    """获取补全脚本的安装路径

    Args:
        shell: Shell 类型 (bash/zsh/fish)

    Returns:
        (安装路径, 路径描述)
    """
    home = os.path.expanduser('~')

    if shell == 'zsh':
        return os.path.join(home, '.zfunc', '_code-stats'), '~/.zfunc/_code-stats'
    elif shell == 'bash':
        return os.path.join(home, '.bash_completion.d', 'code-stats'), '~/.bash_completion.d/code-stats'
    elif shell == 'fish':
        return os.path.join(home, '.config', 'fish', 'completions', 'code-stats.fish'), '~/.config/fish/completions/code-stats.fish'
    else:
        raise ValueError(f"Unsupported shell: {shell}")


def install_completion() -> Tuple[bool, str]:
    """安装补全脚本到当前 Shell 的配置目录

    Returns:
        (成功与否, 消息)
    """
    try:
        import argcomplete
    except ImportError:
        return False, "错误: 需要安装 argcomplete (pip install code-stats[completion])"

    # 检测 shell
    shell = detect_shell()
    if not shell:
        return False, f"错误: 无法检测 Shell 类型，当前 SHELL={os.environ.get('SHELL', 'unset')}"

    # 获取安装路径
    install_path, display_path = get_completion_install_path(shell)

    # 生成补全脚本（executables 参数需要传入列表）
    script = argcomplete.shellcode(['code-stats'], shell=shell)

    # 确保目录存在
    install_dir = os.path.dirname(install_path)
    os.makedirs(install_dir, exist_ok=True)

    # 写入文件
    with open(install_path, 'w') as f:
        f.write(script)

    # 构建提示信息
    msg = f"{shell} completion installed in {display_path}\n"

    # 添加额外配置提示
    if shell == 'zsh':
        msg += "\n请确保 ~/.zshrc 中包含以下配置:\n"
        msg += "  fpath=(~/.zfunc $fpath)\n"
        msg += "  autoload -Uz compinit && compinit\n"
    elif shell == 'bash':
        msg += "\n请确保 ~/.bashrc 中包含以下配置:\n"
        msg += "  for f in ~/.bash_completion.d/*; do source \"$f\"; done\n"

    msg += "\nCompletion will take effect once you restart the terminal"

    return True, msg
