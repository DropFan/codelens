"""Git 仓库信息获取"""

import os
import subprocess
from typing import Any, Dict, Optional


class GitInfoProvider:
    """Git 仓库信息提供者"""

    def __init__(self, verbose: bool = False):
        """
        初始化

        Args:
            verbose: 是否输出详细信息
        """
        self.verbose = verbose

    def get_info(self, repo_path: str) -> Dict[str, Optional[Any]]:
        """获取 Git 仓库信息

        Args:
            repo_path: 仓库路径

        Returns:
            dict: Git 信息字典
        """
        git_info: Dict[str, Optional[Any]] = {
            'branch': None,
            'last_commit_date': None,
            'last_commit_author': None,
            'last_commit_message': None,
            'total_commits': None,
            'contributors': None,
            'remote_url': None
        }

        git_dir = os.path.join(repo_path, '.git')
        if not os.path.exists(git_dir):
            return git_info

        try:
            # 获取当前分支
            result = subprocess.run(
                ['git', '-C', repo_path, 'rev-parse', '--abbrev-ref', 'HEAD'],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                git_info['branch'] = result.stdout.strip()

            # 获取最后一次提交信息
            result = subprocess.run(
                ['git', '-C', repo_path, 'log', '-1', '--format=%ai|%an|%s'],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0 and result.stdout.strip():
                parts = result.stdout.strip().split('|', 2)
                if len(parts) >= 3:
                    git_info['last_commit_date'] = parts[0]
                    git_info['last_commit_author'] = parts[1]
                    git_info['last_commit_message'] = parts[2][:80]  # 截断过长的消息

            # 获取提交总数
            result = subprocess.run(
                ['git', '-C', repo_path, 'rev-list', '--count', 'HEAD'],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                git_info['total_commits'] = int(result.stdout.strip())

            # 获取贡献者数量
            result = subprocess.run(
                ['git', '-C', repo_path, 'shortlog', '-sn', 'HEAD'],
                capture_output=True, text=True, timeout=10
            )
            if result.returncode == 0:
                output = result.stdout.strip()
                git_info['contributors'] = len(output.split('\n')) if output else 0

            # 获取远程仓库 URL
            result = subprocess.run(
                ['git', '-C', repo_path, 'remote', 'get-url', 'origin'],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                git_info['remote_url'] = result.stdout.strip()

        except subprocess.TimeoutExpired:
            if self.verbose:
                print(f"警告：获取 {repo_path} 的 Git 信息超时")
        except Exception as e:
            if self.verbose:
                print(f"警告：获取 {repo_path} 的 Git 信息失败: {e}")

        return git_info
