"""GitIgnore 规则解析和匹配"""

import os
import subprocess
from typing import Any, Dict, List, Optional

# 尝试导入 pathspec，用于解析 .gitignore 规则（可选依赖）
try:
    import pathspec  # type: ignore[import-not-found]
    HAS_PATHSPEC = True
except ImportError:
    pathspec = None  # type: ignore[assignment]
    HAS_PATHSPEC = False


class GitIgnoreMatcher:
    """解析和匹配 .gitignore 规则的类"""

    def __init__(self, repo_path: str):
        """
        初始化 GitIgnoreMatcher

        Args:
            repo_path: Git 仓库根目录路径
        """
        self.repo_path = os.path.abspath(repo_path)
        self.enabled = False
        self.use_git_command = False
        self._pathspec: Optional[Any] = None  # pathspec.PathSpec when available
        self._git_check_cache: Dict[str, bool] = {}
        self._gitignore_patterns: List[str] = []

        # 检查是否是 Git 仓库
        if not os.path.isdir(os.path.join(self.repo_path, '.git')):
            return

        # 优先使用 pathspec 库
        if HAS_PATHSPEC:
            self._load_gitignore_with_pathspec()
        else:
            # 检查 git 命令是否可用
            try:
                result = subprocess.run(
                    ['git', '--version'],
                    capture_output=True,
                    text=True,
                    timeout=5
                )
                if result.returncode == 0:
                    self.use_git_command = True
                    self.enabled = True
            except (subprocess.SubprocessError, FileNotFoundError):
                pass

    def _load_gitignore_with_pathspec(self) -> None:
        """使用 pathspec 库加载所有 .gitignore 规则"""
        patterns: List[str] = []

        # 加载全局 gitignore（如果有）
        global_gitignore = self._get_global_gitignore()
        if global_gitignore and os.path.isfile(global_gitignore):
            patterns.extend(self._read_gitignore_file(global_gitignore))

        # 加载仓库根目录的 .gitignore
        root_gitignore = os.path.join(self.repo_path, '.gitignore')
        if os.path.isfile(root_gitignore):
            patterns.extend(self._read_gitignore_file(root_gitignore))

        # 递归加载子目录的 .gitignore
        for root, dirs, files in os.walk(self.repo_path):
            # 跳过 .git 目录
            if '.git' in dirs:
                dirs.remove('.git')

            if '.gitignore' in files and root != self.repo_path:
                gitignore_path = os.path.join(root, '.gitignore')
                rel_dir = os.path.relpath(root, self.repo_path)
                sub_patterns = self._read_gitignore_file(gitignore_path, prefix=rel_dir)
                patterns.extend(sub_patterns)

        if patterns and HAS_PATHSPEC and pathspec is not None:
            self._pathspec = pathspec.PathSpec.from_lines('gitwildmatch', patterns)  # type: ignore[union-attr]
            self.enabled = True
            self._gitignore_patterns = patterns

    def _get_global_gitignore(self) -> Optional[str]:
        """获取全局 gitignore 文件路径"""
        try:
            result = subprocess.run(
                ['git', 'config', '--global', 'core.excludesFile'],
                capture_output=True,
                text=True,
                timeout=5,
                cwd=self.repo_path
            )
            if result.returncode == 0 and result.stdout.strip():
                path = result.stdout.strip()
                # 展开 ~ 路径
                return os.path.expanduser(path)
        except (subprocess.SubprocessError, FileNotFoundError):
            pass

        # 检查默认位置
        default_paths = [
            os.path.expanduser('~/.gitignore_global'),
            os.path.expanduser('~/.gitignore'),
            os.path.expanduser('~/.config/git/ignore')
        ]
        for path in default_paths:
            if os.path.isfile(path):
                return path
        return None

    def _read_gitignore_file(
        self, filepath: str, prefix: Optional[str] = None
    ) -> List[str]:
        """读取 .gitignore 文件并返回规则列表"""
        patterns: List[str] = []
        try:
            with open(filepath, 'r', encoding='utf-8', errors='ignore') as f:
                for line in f:
                    line = line.rstrip('\r\n')
                    # 跳过空行和注释
                    if not line or line.startswith('#'):
                        continue
                    # 处理带前缀的子目录规则
                    if prefix:
                        # 如果规则以 / 开头，表示相对于 .gitignore 所在目录
                        if line.startswith('/'):
                            patterns.append(prefix + line)
                        elif line.startswith('!'):
                            # 否定规则
                            if line[1:].startswith('/'):
                                patterns.append('!' + prefix + line[1:])
                            else:
                                patterns.append('!' + prefix + '/' + line[1:])
                        else:
                            patterns.append(prefix + '/' + line)
                    else:
                        patterns.append(line)
        except (IOError, OSError):
            pass
        return patterns

    def is_ignored(self, path: str, is_dir: bool = False) -> bool:
        """
        检查路径是否应该被忽略

        Args:
            path: 相对于仓库根目录的路径或绝对路径
            is_dir: 是否是目录

        Returns:
            bool: True 如果应该被忽略
        """
        if not self.enabled:
            return False

        # 转换为相对路径
        if os.path.isabs(path):
            try:
                rel_path = os.path.relpath(path, self.repo_path)
            except ValueError:
                return False
        else:
            rel_path = path

        # 规范化路径分隔符
        rel_path = rel_path.replace(os.sep, '/')

        # 移除开头的 ./
        if rel_path.startswith('./'):
            rel_path = rel_path[2:]

        # 如果路径在仓库外，不忽略
        if rel_path.startswith('..'):
            return False

        # 使用 pathspec 检查
        if self._pathspec is not None:
            # 对于目录，需要在末尾加 /
            check_path = rel_path + '/' if is_dir else rel_path
            return self._pathspec.match_file(check_path)

        # 使用 git check-ignore 命令
        if self.use_git_command:
            return self._check_with_git_command(rel_path)

        return False

    def _check_with_git_command(self, rel_path: str) -> bool:
        """使用 git check-ignore 命令检查路径"""
        # 检查缓存
        if rel_path in self._git_check_cache:
            return self._git_check_cache[rel_path]

        try:
            result = subprocess.run(
                ['git', 'check-ignore', '-q', rel_path],
                capture_output=True,
                cwd=self.repo_path,
                timeout=5
            )
            is_ignored = result.returncode == 0
            self._git_check_cache[rel_path] = is_ignored
            return is_ignored
        except (subprocess.SubprocessError, FileNotFoundError):
            return False

    def batch_check(self, paths: List[str]) -> Dict[str, bool]:
        """
        批量检查多个路径是否被忽略

        Args:
            paths: 路径列表（相对路径或绝对路径）

        Returns:
            dict: {path: is_ignored}
        """
        if not self.enabled or not paths:
            return {p: False for p in paths}

        results: Dict[str, bool] = {}

        if self._pathspec is not None:
            for path in paths:
                results[path] = self.is_ignored(path)
        elif self.use_git_command:
            # 使用 git check-ignore --stdin 批量检查
            uncached: List[tuple] = []
            for path in paths:
                if os.path.isabs(path):
                    try:
                        rel_path = os.path.relpath(path, self.repo_path)
                    except ValueError:
                        results[path] = False
                        continue
                else:
                    rel_path = path

                if rel_path in self._git_check_cache:
                    results[path] = self._git_check_cache[rel_path]
                else:
                    uncached.append((path, rel_path))

            if uncached:
                try:
                    input_text = '\n'.join(rel for _, rel in uncached)
                    result = subprocess.run(
                        ['git', 'check-ignore', '--stdin'],
                        input=input_text,
                        capture_output=True,
                        text=True,
                        cwd=self.repo_path,
                        timeout=30
                    )
                    ignored_set = (
                        set(result.stdout.strip().split('\n'))
                        if result.stdout.strip()
                        else set()
                    )

                    for path, rel_path in uncached:
                        is_ignored = rel_path in ignored_set
                        self._git_check_cache[rel_path] = is_ignored
                        results[path] = is_ignored
                except (subprocess.SubprocessError, FileNotFoundError):
                    for path, _ in uncached:
                        results[path] = False
        else:
            results = {p: False for p in paths}

        return results
