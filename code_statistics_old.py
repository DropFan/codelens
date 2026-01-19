#!/usr/bin/env python3
"""
代码统计脚本 - 统计当前目录下所有仓库的代码行数
支持多种参数自定义统计行为
"""

__version__ = "1.6.0"

import os
import argparse
from pathlib import Path
from collections import defaultdict
import json
import csv
import fnmatch
import concurrent.futures
from datetime import datetime
import time
import re
import subprocess

# 尝试导入 yaml，如果不存在则提供降级方案
try:
    import yaml
    HAS_YAML = True
except ImportError:
    HAS_YAML = False

# 尝试导入 pathspec，用于解析 .gitignore 规则
try:
    import pathspec
    HAS_PATHSPEC = True
except ImportError:
    HAS_PATHSPEC = False

# 基础排除目录（所有项目通用）
BASE_EXCLUDE_DIRS = {
    # 版本控制
    '.git', '.svn', '.hg', '.bzr', '_darcs',

    # IDE 和编辑器
    '.idea', '.vscode', '.vs', '.settings', '.project',

    # 临时文件和日志
    'logs', 'log', 'tmp', 'temp', '.cache',
    '.DS_Store', 'Thumbs.db', 'desktop.ini',
    '*.swp', '*.swo', '*~',

    # 基础设施
    '.terraform', '.vagrant', '.docker'
}

# 按语言/框架的排除目录
LANGUAGE_EXCLUDE_DIRS = {
    'python': {
        # 虚拟环境
        '__pycache__', '.venv', 'venv', 'env', '.env', 'virtualenv',
        '.conda', 'envs', '__pypackages__',
        # 测试和类型检查缓存
        '.pytest_cache', '.mypy_cache', '.pytype', '.pyre',
        '.tox', '.nox', '.hypothesis',
        # 打包和分发
        '*.egg-info', '.eggs', 'site-packages', 'dist', 'build',
        # 覆盖率报告
        'htmlcov', '.coverage',
        # 其他工具缓存
        '.ruff_cache', '.pdm-build', '.pdm-python',
    },
    'javascript': {
        # 包管理器
        'node_modules', '.npm', 'bower_components', 'jspm_packages',
        '.yarn', '.pnp', '.pnpm-store',
        # 测试覆盖率
        'coverage', '.nyc_output',
        # 框架构建目录
        '.next', '.nuxt', '.parcel-cache', '.gatsby',
        '.turbo', '.vercel', '.netlify', '.sveltekit',
        '.docusaurus', '.vuepress', '.vitepress',
        # 构建输出
        'dist', 'build', 'out', 'lib',
    },
    'typescript': {
        # 包管理器
        'node_modules', '.npm', 'bower_components',
        '.yarn', '.pnp', '.pnpm-store',
        # 测试覆盖率
        'coverage', '.nyc_output',
        # 框架构建目录
        '.next', '.nuxt', '.parcel-cache', '.gatsby',
        '.turbo', '.vercel', '.netlify', '.sveltekit',
        # 构建输出
        'dist', 'build', 'out', 'lib',
    },
    'go': {
        'vendor',  # Go modules vendor 目录
        'bin',     # 常见输出目录
    },
    'java': {
        # Maven/Gradle 构建输出
        'target', 'build', 'out', 'classes',
        '.gradle', '.mvn', '.m2',
        # 生成的代码
        'generated', 'generated-sources', 'generated-test-sources',
        # Eclipse
        '.settings', 'bin',
    },
    'kotlin': {
        'target', 'build', 'out', 'classes',
        '.gradle', '.mvn', '.m2',
        'generated', '.kotlin',
    },
    'scala': {
        'target', 'project/target', 'project/project',
        '.bloop', '.metals', '.bsp',
        '.sbt', '.ivy2',
    },
    'csharp': {
        'bin', 'obj', '.vs', 'packages', '.nuget',
        'TestResults', '.fake', 'paket-files',
        '_ReSharper*', '*.resharper*',
    },
    'fsharp': {
        'bin', 'obj', '.vs', 'packages', '.nuget',
        '.fake', 'paket-files',
    },
    'ruby': {
        '.bundle', 'vendor/bundle', 'vendor/cache',
        '.sass-cache', '_site',
        'coverage', 'tmp', 'log',
    },
    'php': {
        'vendor',  # Composer 依赖
        # Laravel 框架
        'storage/framework', 'bootstrap/cache',
        # 测试和静态分析缓存
        '.phpunit.result.cache', '.phpunit.cache',
        '.php_cs.cache', '.php-cs-fixer.cache',
        '.phpstan', '.psalm',
    },
    'rust': {
        'target',  # Cargo 构建输出
        '.cargo',  # Cargo 缓存
    },
    'dart': {
        '.dart_tool', 'build',
        # Flutter
        '.flutter-plugins', '.flutter-plugins-dependencies',
        '.pub-cache', '.pub',
        'ios/Pods', 'android/.gradle',
    },
    'swift': {
        'Pods', '.build', 'Build',
        'xcuserdata', '*.xcworkspace',
        'DerivedData', '.swiftpm',
    },
    'objc': {
        'Pods', '.build', 'Build',
        'xcuserdata', '*.xcworkspace',
        'DerivedData',
    },
    'c': {
        # 构建输出
        'build', 'bin', 'obj', 'out', 'lib',
        # CMake
        'CMakeFiles', 'cmake-build-*',
        # Make
        '*.o', '*.a', '*.so', '*.dylib',
    },
    'cpp': {
        # 构建输出
        'build', 'bin', 'obj', 'out', 'lib',
        # CMake
        'CMakeFiles', 'cmake-build-*',
        # Make
        '*.o', '*.a', '*.so', '*.dylib',
        # vcpkg
        'vcpkg_installed',
    },
    'elixir': {
        '_build', 'deps', '.fetch',
        'cover', 'doc',
    },
    'erlang': {
        '_build', 'deps', '.fetch',
        '_rel', 'log',
    },
    'haskell': {
        '.stack-work', 'dist', 'dist-newstyle',
        '.cabal-sandbox', '.ghc.environment.*',
    },
    'lua': {
        'lua_modules', '.luarocks',
    },
    'perl': {
        'blib', '_build', 'Build',
        'local', 'fatlib',
    },
    'r': {
        'packrat', 'renv',
        '.Rproj.user', 'rsconnect',
    },
    'julia': {
        '.julia', 'deps/build',
    },
    'clojure': {
        'target', '.cpcache', '.lsp', '.clj-kondo',
    },
}

# 项目类型检测标记文件
PROJECT_MARKERS = {
    'python': ['requirements.txt', 'setup.py', 'pyproject.toml', 'Pipfile', 'setup.cfg', 'poetry.lock'],
    'javascript': ['package.json', 'yarn.lock', 'package-lock.json', 'pnpm-lock.yaml'],
    'typescript': ['tsconfig.json'],
    'go': ['go.mod', 'go.sum'],
    'java': ['pom.xml', 'build.gradle', 'build.gradle.kts'],
    'kotlin': ['build.gradle.kts', '*.kt'],
    'scala': ['build.sbt', '*.scala'],
    'csharp': ['*.csproj', '*.sln', 'packages.config', 'global.json'],
    'fsharp': ['*.fsproj', '*.sln'],
    'ruby': ['Gemfile', 'Rakefile', '*.gemspec'],
    'php': ['composer.json', 'composer.lock', 'artisan'],
    'rust': ['Cargo.toml', 'Cargo.lock'],
    'dart': ['pubspec.yaml', 'pubspec.lock'],
    'swift': ['Package.swift', '*.xcodeproj', '*.xcworkspace'],
    'objc': ['*.xcodeproj', '*.xcworkspace', 'Podfile'],
    'c': ['CMakeLists.txt', 'Makefile', '*.c'],
    'cpp': ['CMakeLists.txt', 'Makefile', '*.cpp', '*.cc', '*.cxx'],
    'elixir': ['mix.exs', 'mix.lock'],
    'erlang': ['rebar.config', 'rebar.lock'],
    'haskell': ['stack.yaml', '*.cabal', 'cabal.project'],
    'lua': ['*.lua', '.luacheckrc'],
    'perl': ['Makefile.PL', 'Build.PL', 'cpanfile'],
    'r': ['DESCRIPTION', '*.Rproj'],
    'julia': ['Project.toml', 'Manifest.toml'],
    'clojure': ['project.clj', 'deps.edn'],
}

# 为了向后兼容，保留 DEFAULT_EXCLUDE_DIRS（合并所有语言的排除目录）
DEFAULT_EXCLUDE_DIRS = BASE_EXCLUDE_DIRS.copy()
for lang_excludes in LANGUAGE_EXCLUDE_DIRS.values():
    DEFAULT_EXCLUDE_DIRS.update(lang_excludes)

# 文档文件扩展名
DOC_EXTENSIONS = {
    '.md', '.markdown', '.rst', '.txt', '.doc', '.docx',
    '.pdf', '.odt', '.rtf', '.tex', '.wiki', '.org',
    '.adoc', '.asciidoc', '.pod', '.man', '.textile',
    'readme', 'license', 'changelog', 'authors', 'contributors',
    'notice', 'history', 'changes', 'install', 'todo'
}

# 配置文件扩展名（需要排除）
CONFIG_EXTENSIONS = {
    '.yml', '.yaml', '.json', '.xml', '.toml', '.ini',
    '.cfg', '.conf', '.config', '.properties', '.props',
    '.env', '.env.example', '.env.sample', '.env.local',
    
    # 锁文件和日志
    '.lock', '.log', '.pid', '.seed', '.csv', '.tsv',
    
    # 忽略文件
    '.gitignore', '.dockerignore', '.editorconfig',
    '.gitattributes', '.npmignore', '.prettierignore',
    '.eslintignore', '.stylelintignore',
    
    # 其他
    'LICENSE', 'README', 'CHANGELOG', 'TODO',
    'AUTHORS', 'CONTRIBUTORS', 'NOTICE', 'PATENTS'
}

# 二进制文件扩展名（需要排除）
BINARY_EXTENSIONS = {
    # 可执行文件
    '.exe', '.dll', '.so', '.dylib', '.lib', '.a', '.o', '.obj',
    '.class', '.jar', '.war', '.ear', '.pyc', '.pyo', '.pyd',
    '.wasm', '.bc', '.out', '.app', '.elf', '.deb', '.rpm',
    
    # 压缩文件
    '.zip', '.rar', '.7z', '.tar', '.gz', '.bz2', '.xz', '.tgz',
    '.lz', '.lzma', '.z', '.Z', '.dz', '.bz', '.tbz', '.tbz2',
    '.taz', '.tlz', '.txz', '.tzo', '.lz4', '.zst', '.zstd',
    
    # 图片文件
    '.jpg', '.jpeg', '.png', '.gif', '.bmp', '.ico', '.svg',
    '.webp', '.tiff', '.tif', '.psd', '.raw', '.heif', '.heic',
    '.dng', '.cr2', '.nef', '.arw', '.rw2', '.raf', '.orf',
    
    # 音频文件
    '.mp3', '.wav', '.flac', '.aac', '.ogg', '.wma', '.m4a',
    '.mp2', '.mp1', '.opus', '.vorbis', '.amr', '.ac3', '.ec3',
    '.mka', '.m3u', '.m3u8', '.pls', '.cue',
    
    # 视频文件
    '.mp4', '.avi', '.mkv', '.mov', '.wmv', '.flv', '.webm',
    '.m4v', '.mpg', '.mpeg', '.3gp', '.3g2', '.mxf', '.ts',
    '.m2ts', '.vob', '.ogv', '.drc', '.mng', '.qt', '.yuv',
    '.rm', '.rmvb', '.asf', '.amv', '.m4p', '.m4v', '.svi',
    
    # 字体文件
    '.ttf', '.otf', '.woff', '.woff2', '.eot', '.sfnt', '.fon',
    '.fnt', '.font', '.ttc', '.pfb', '.pfm', '.afm',
    
    # 数据库文件
    '.db', '.sqlite', '.sqlite3', '.mdb', '.accdb', '.dbf',
    
    # 办公文档（通常是二进制）
    '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx',
    '.odt', '.ods', '.odp', '.pdf',
    
    # 模型和数据文件
    '.pkl', '.pickle', '.npy', '.npz', '.h5', '.hdf5', '.mat',
    '.model', '.weights', '.pb', '.pth', '.pt', '.onnx',
    '.safetensors', '.ckpt', '.bin', '.dat', '.data',
    
    # Git 对象
    '.pack', '.idx',
    
    # 其他二进制格式
    '.iso', '.dmg', '.pkg', '.msi', '.apk', '.ipa', '.dex',
    '.bundle', '.framework', '.xcframework', '.aar',
    '.rdb', '.aof', '.dump', '.mem', '.swap', '.core',
    '.DS_Store', '.localized', '.Spotlight-V100', '.Trashes',
    '.fseventsd', '.hotfiles.btree', '.DocumentRevisions-V100',
    '.TemporaryItems', '.apdisk', '.VolumeIcon.icns'
}

# 代码文件扩展名（包含）
CODE_EXTENSIONS = {
    # Python
    '.py', '.pyw', '.pyx', '.pxd', '.pxi', '.pyi',
    
    # Go
    '.go', '.mod',
    
    # JavaScript/TypeScript/Node
    '.js', '.jsx', '.ts', '.tsx', '.mjs', '.cjs', '.es6',
    '.coffee', '.litcoffee',
    
    # Java/JVM Languages
    '.java', '.kt', '.kts', '.scala', '.sc', '.groovy', '.gvy',
    '.clj', '.cljs', '.cljc', '.edn',
    
    # C/C++/Objective-C
    '.c', '.cc', '.cpp', '.cxx', '.c++', '.h', '.hh', '.hpp', '.hxx', '.h++',
    '.ino', '.cu', '.cuh', '.m', '.mm',
    
    # C#/.NET
    '.cs', '.csx', '.vb', '.fs', '.fsi', '.fsx', '.fsscript',
    
    # Ruby
    '.rb', '.rbw', '.rake', '.gemspec', '.ru', '.erb', '.haml', '.slim',
    
    # PHP
    '.php', '.phtml', '.php3', '.php4', '.php5', '.php7', '.php8', '.phps',
    
    # Rust
    '.rs',
    
    # Swift
    '.swift',
    
    # Shell/Bash
    '.sh', '.bash', '.zsh', '.fish', '.ksh', '.csh', '.tcsh',
    '.ps1', '.psm1', '.psd1', '.ps1xml', '.pssc', '.bat', '.cmd',
    
    # Web Frontend
    '.html', '.htm', '.xhtml', '.xml', '.css', '.scss', '.sass', '.less', '.styl',
    '.vue', '.svelte', '.astro', '.jsx', '.tsx',
    
    # Mobile
    '.dart', '.gradle', '.pro',
    
    # Database
    '.sql', '.plsql', '.tsql', '.psql', '.mysql',
    
    # Data Science / ML
    '.r', '.R', '.rmd', '.Rmd', '.jl', '.ipynb',
    '.mat', '.m', '.mlx',
    
    # Functional Programming
    '.hs', '.lhs', '.elm', '.ml', '.mli', '.fs', '.fsi', '.fsx',
    '.erl', '.hrl', '.ex', '.exs', '.nim', '.nims',
    
    # Systems Programming
    '.asm', '.s', '.S', '.nasm', '.v', '.vhd', '.vhdl',
    
    # Scripting
    '.pl', '.pm', '.pod', '.t', '.lua', '.tcl', '.awk',
    '.sed', '.vim', '.vimrc', '.emacs', '.el',
    
    # Configuration as Code
    '.tf', '.tfvars', '.hcl', '.nomad', '.workflow', '.wdl',
    
    # Other Languages
    '.pas', '.pp', '.inc', '.d', '.di', '.zig', '.odin',
    '.ada', '.adb', '.ads', '.f', '.f90', '.f95', '.f03',
    '.cob', '.cbl', '.lisp', '.lsp', '.cl', '.rkt', '.scm',
    '.pro', '.P', '.ecl', '.ncl', '.sml', '.sig',
    
    # Markup/Template
    '.jsp', '.asp', '.aspx', '.ejs', '.pug', '.jade', '.hbs',
    '.mustache', '.twig', '.liquid', '.jinja', '.j2',
    
    # Build/Make
    '.cmake', '.mk', '.mak', '.gnumakefile', '.makefile',
    '.ninja', '.gn', '.gni', '.bazel', '.bzl', '.BUILD',
    
    # Documentation (that contains code)
    '.tex', '.cls', '.sty', '.bib',
    
    # Game Development
    '.cs', '.shader', '.cginc', '.hlsl', '.glsl', '.vert', '.frag',
    '.metal', '.wgsl',
    
    # Smart Contracts
    '.sol', '.vy', '.yul', '.move'
}

# 语言映射
LANGUAGE_MAP = {
    # Python
    '.py': 'python', '.pyw': 'python', '.pyx': 'python',
    '.pxd': 'python', '.pxi': 'python', '.pyi': 'python',
    
    # Go
    '.go': 'go', '.mod': 'go',
    
    # JavaScript/TypeScript
    '.js': 'javascript', '.jsx': 'javascript', '.mjs': 'javascript',
    '.cjs': 'javascript', '.es6': 'javascript',
    '.ts': 'typescript', '.tsx': 'typescript',
    '.coffee': 'coffeescript', '.litcoffee': 'coffeescript',
    
    # Java/JVM
    '.java': 'java',
    '.kt': 'kotlin', '.kts': 'kotlin',
    '.scala': 'scala', '.sc': 'scala',
    '.groovy': 'groovy', '.gvy': 'groovy',
    '.clj': 'clojure', '.cljs': 'clojure', '.cljc': 'clojure',
    
    # C/C++/Objective-C
    '.c': 'c', '.h': 'c',
    '.cc': 'cpp', '.cpp': 'cpp', '.cxx': 'cpp', '.c++': 'cpp',
    '.hh': 'cpp', '.hpp': 'cpp', '.hxx': 'cpp', '.h++': 'cpp',
    '.cu': 'cuda', '.cuh': 'cuda',
    '.m': 'objc', '.mm': 'objcpp',
    '.ino': 'arduino',
    
    # C#/.NET
    '.cs': 'csharp', '.csx': 'csharp',
    '.vb': 'vbnet',
    '.fs': 'fsharp', '.fsi': 'fsharp', '.fsx': 'fsharp',
    
    # Ruby
    '.rb': 'ruby', '.rbw': 'ruby', '.rake': 'ruby',
    '.gemspec': 'ruby', '.ru': 'ruby',
    '.erb': 'erb', '.haml': 'haml', '.slim': 'slim',
    
    # PHP
    '.php': 'php', '.phtml': 'php', '.php3': 'php',
    '.php4': 'php', '.php5': 'php', '.php7': 'php',
    '.php8': 'php', '.phps': 'php',
    
    # Rust
    '.rs': 'rust',
    
    # Swift
    '.swift': 'swift',
    
    # Shell
    '.sh': 'shell', '.bash': 'shell', '.zsh': 'shell',
    '.fish': 'shell', '.ksh': 'shell', '.csh': 'shell',
    '.tcsh': 'shell',
    '.ps1': 'powershell', '.psm1': 'powershell', '.psd1': 'powershell',
    '.bat': 'batch', '.cmd': 'batch',
    
    # Web
    '.html': 'html', '.htm': 'html', '.xhtml': 'html',
    '.xml': 'xml',
    '.css': 'css', '.scss': 'scss', '.sass': 'sass',
    '.less': 'less', '.styl': 'stylus',
    '.vue': 'vue', '.svelte': 'svelte', '.astro': 'astro',
    
    # Mobile
    '.dart': 'dart',
    '.gradle': 'gradle',
    
    # Database
    '.sql': 'sql', '.plsql': 'plsql', '.tsql': 'tsql',
    '.psql': 'postgresql', '.mysql': 'mysql',
    
    # Data Science
    '.r': 'r', '.R': 'r', '.rmd': 'rmarkdown', '.Rmd': 'rmarkdown',
    '.jl': 'julia',
    '.ipynb': 'jupyter',
    '.mat': 'matlab', '.m': 'matlab', '.mlx': 'matlab',
    
    # Functional
    '.hs': 'haskell', '.lhs': 'haskell',
    '.elm': 'elm',
    '.ml': 'ocaml', '.mli': 'ocaml',
    '.erl': 'erlang', '.hrl': 'erlang',
    '.ex': 'elixir', '.exs': 'elixir',
    '.nim': 'nim', '.nims': 'nim',
    
    # Systems
    '.asm': 'assembly', '.s': 'assembly', '.S': 'assembly',
    '.nasm': 'nasm',
    '.v': 'verilog', '.vhd': 'vhdl', '.vhdl': 'vhdl',
    
    # Scripting
    '.pl': 'perl', '.pm': 'perl', '.pod': 'perl', '.t': 'perl',
    '.lua': 'lua',
    '.tcl': 'tcl',
    '.awk': 'awk',
    '.vim': 'viml', '.vimrc': 'viml',
    '.el': 'elisp', '.emacs': 'elisp',
    
    # Configuration as Code
    '.tf': 'terraform', '.tfvars': 'terraform',
    '.hcl': 'hcl', '.nomad': 'hcl',
    '.workflow': 'github-actions', '.wdl': 'wdl',
    
    # Other Languages
    '.pas': 'pascal', '.pp': 'pascal', '.inc': 'pascal',
    '.d': 'd', '.di': 'd',
    '.zig': 'zig',
    '.odin': 'odin',
    '.ada': 'ada', '.adb': 'ada', '.ads': 'ada',
    '.f': 'fortran', '.f90': 'fortran', '.f95': 'fortran',
    '.cob': 'cobol', '.cbl': 'cobol',
    '.lisp': 'lisp', '.lsp': 'lisp', '.cl': 'commonlisp',
    '.rkt': 'racket', '.scm': 'scheme',
    '.pro': 'prolog', '.P': 'prolog',
    '.ecl': 'ecl', '.ncl': 'ncl',
    '.sml': 'sml', '.sig': 'sml',
    
    # Templates
    '.jsp': 'jsp', '.asp': 'asp', '.aspx': 'aspx',
    '.ejs': 'ejs', '.pug': 'pug', '.jade': 'jade',
    '.hbs': 'handlebars', '.mustache': 'mustache',
    '.twig': 'twig', '.liquid': 'liquid',
    '.jinja': 'jinja', '.j2': 'jinja',
    
    # Build
    '.cmake': 'cmake', '.mk': 'make', '.mak': 'make',
    '.ninja': 'ninja', '.gn': 'gn', '.gni': 'gn',
    '.bazel': 'bazel', '.bzl': 'bazel', '.BUILD': 'bazel',
    
    # Documentation
    '.tex': 'latex', '.cls': 'latex', '.sty': 'latex',
    '.bib': 'bibtex',
    
    # Game Development
    '.shader': 'shaderlab', '.cginc': 'shaderlab',
    '.hlsl': 'hlsl', '.glsl': 'glsl',
    '.vert': 'glsl', '.frag': 'glsl',
    '.metal': 'metal', '.wgsl': 'wgsl',
    
    # Smart Contracts
    '.sol': 'solidity', '.vy': 'vyper', '.yul': 'yul',
    '.move': 'move'
}


class GitIgnoreMatcher:
    """解析和匹配 .gitignore 规则的类"""

    def __init__(self, repo_path):
        """
        初始化 GitIgnoreMatcher

        Args:
            repo_path: Git 仓库根目录路径
        """
        self.repo_path = os.path.abspath(repo_path)
        self.enabled = False
        self.use_git_command = False
        self._pathspec = None
        self._git_check_cache = {}
        self._gitignore_patterns = []

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

    def _load_gitignore_with_pathspec(self):
        """使用 pathspec 库加载所有 .gitignore 规则"""
        patterns = []

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

        if patterns:
            self._pathspec = pathspec.PathSpec.from_lines('gitwildmatch', patterns)
            self.enabled = True
            self._gitignore_patterns = patterns

    def _get_global_gitignore(self):
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

    def _read_gitignore_file(self, filepath, prefix=None):
        """读取 .gitignore 文件并返回规则列表"""
        patterns = []
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

    def is_ignored(self, path, is_dir=False):
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

    def _check_with_git_command(self, rel_path):
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

    def batch_check(self, paths):
        """
        批量检查多个路径是否被忽略

        Args:
            paths: 路径列表（相对路径或绝对路径）

        Returns:
            dict: {path: is_ignored}
        """
        if not self.enabled or not paths:
            return {p: False for p in paths}

        results = {}

        if self._pathspec is not None:
            for path in paths:
                results[path] = self.is_ignored(path)
        elif self.use_git_command:
            # 使用 git check-ignore --stdin 批量检查
            uncached = []
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
                    ignored_set = set(result.stdout.strip().split('\n')) if result.stdout.strip() else set()

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


class CodeStatistics:
    def __init__(self, args):
        self.args = args
        # 使用基础排除目录，完整排除列表将在 analyze_repository 中动态构建
        self.base_exclude_dirs = set(BASE_EXCLUDE_DIRS)
        self.user_exclude_dirs = set()  # 用户指定的额外排除目录
        self.exclude_dirs = set()  # 运行时使用的排除目录（会在 analyze_repository 中更新）
        self.current_dir = os.getcwd()
        self.progress_count = 0
        self.total_repos = 0

        # 处理额外的排除模式
        if args.excludes:
            for pattern in args.excludes.split(','):
                self.user_exclude_dirs.add(pattern.strip())
        
        # 处理语言过滤
        self.target_languages = None
        if args.lang:
            self.target_languages = set(lang.strip().lower() for lang in args.lang.split(','))
        
        # 处理正则表达式排除模式
        self.exclude_file_patterns = []
        if args.exclude_files:
            for pattern in args.exclude_files.split(','):
                try:
                    self.exclude_file_patterns.append(re.compile(pattern.strip()))
                except re.error as e:
                    print(f"警告：无效的文件排除正则表达式 '{pattern}': {e}")
        
        self.exclude_dir_patterns = []
        if args.exclude_dirs:
            for pattern in args.exclude_dirs.split(','):
                try:
                    self.exclude_dir_patterns.append(re.compile(pattern.strip()))
                except re.error as e:
                    print(f"警告：无效的目录排除正则表达式 '{pattern}': {e}")

        # 处理文件包含模式
        self.include_file_patterns = []
        if getattr(args, 'include_files', None):
            for pattern in args.include_files.split(','):
                try:
                    self.include_file_patterns.append(re.compile(pattern.strip()))
                except re.error as e:
                    print(f"警告：无效的文件包含正则表达式 '{pattern}': {e}")

        # 处理深度限制
        self.max_depth = getattr(args, 'depth', None) or 0  # 0 表示无限制

        # Git 信息选项
        self.git_info = getattr(args, 'git_info', False)

        # Gitignore 过滤选项
        self.use_gitignore = not getattr(args, 'no_gitignore', False)
        self.gitignore_matcher = None  # 当前仓库的 gitignore matcher
    
    def format_size(self, size_in_bytes):
        """格式化文件大小"""
        if size_in_bytes < 1024:
            return f"{size_in_bytes} B"
        elif size_in_bytes < 1024 * 1024:
            return f"{size_in_bytes / 1024:.1f} KB"
        elif size_in_bytes < 1024 * 1024 * 1024:
            return f"{size_in_bytes / (1024 * 1024):.1f} MB"
        else:
            return f"{size_in_bytes / (1024 * 1024 * 1024):.1f} GB"
    
    def get_language_emoji(self, language):
        """获取语言对应的emoji图标"""
        emoji_map = {
            'python': '🐍',
            'javascript': '📜',
            'typescript': '📘',
            'java': '☕',
            'go': '🐹',
            'rust': '🦀',
            'cpp': '⚙️',
            'c': '🔧',
            'csharp': '🔷',
            'ruby': '💎',
            'php': '🐘',
            'swift': '🦉',
            'kotlin': '🟣',
            'scala': '🔴',
            'r': '📊',
            'shell': '🐚',
            'html': '🌐',
            'css': '🎨',
            'sql': '🗄️',
            'dart': '🎯',
            'vue': '💚',
            'react': '⚛️',
            'docker': '🐳',
            'yaml': '📝',
            'json': '📋',
            'xml': '📄',
            'markdown': '📑',
            'terraform': '🏗️',
            'kubernetes': '☸️',
            'haskell': '🎓',
            'elixir': '💧',
            'erlang': '📡',
            'julia': '🟢',
            'matlab': '🔬',
            'perl': '🐪',
            'lua': '🌙',
            'nim': '👑',
            'zig': '⚡',
            'assembly': '🔩',
            'fortran': '🏛️',
            'cobol': '🏦',
            'pascal': '🔺',
            'lisp': '🎭',
            'clojure': '☯️',
            'ocaml': '🐫',
            'fsharp': '📐',
            'vbnet': '🔵',
            'powershell': '💠',
            'batch': '📦',
            'make': '🔨',
            'cmake': '🛠️',
            'gradle': '🐘',
            'maven': '🏗️',
            'unknown': '❓',
            'other': '📌'
        }
        return emoji_map.get(language.lower(), '📄')

    def detect_project_types(self, repo_path):
        """检测仓库中存在的项目类型/语言

        Args:
            repo_path: 仓库路径

        Returns:
            set: 检测到的语言/框架类型集合
        """
        detected_types = set()

        try:
            # 检查根目录的标记文件
            for lang, markers in PROJECT_MARKERS.items():
                for marker in markers:
                    if '*' in marker:
                        # 通配符模式，检查匹配的文件
                        for item in os.listdir(repo_path):
                            if fnmatch.fnmatch(item, marker):
                                detected_types.add(lang)
                                break
                    else:
                        # 精确匹配
                        marker_path = os.path.join(repo_path, marker)
                        if os.path.exists(marker_path):
                            detected_types.add(lang)
                            break

            # 如果没有检测到，扫描文件扩展名来推断
            if not detected_types:
                ext_count = defaultdict(int)
                scan_limit = 100  # 只扫描前100个文件
                file_count = 0

                for root, dirs, files in os.walk(repo_path):
                    # 跳过隐藏目录和基础排除目录
                    dirs[:] = [d for d in dirs if not d.startswith('.') and d not in BASE_EXCLUDE_DIRS]

                    for file in files:
                        if file_count >= scan_limit:
                            break
                        ext = Path(file).suffix.lower()
                        if ext and ext not in BINARY_EXTENSIONS and ext not in CONFIG_EXTENSIONS:
                            ext_count[ext] += 1
                        file_count += 1

                    if file_count >= scan_limit:
                        break

                # 根据文件扩展名推断语言
                ext_to_lang = {
                    '.py': 'python', '.pyw': 'python', '.pyx': 'python',
                    '.js': 'javascript', '.mjs': 'javascript', '.cjs': 'javascript', '.jsx': 'javascript',
                    '.ts': 'typescript', '.tsx': 'typescript', '.mts': 'typescript',
                    '.go': 'go',
                    '.java': 'java',
                    '.kt': 'kotlin', '.kts': 'kotlin',
                    '.scala': 'scala', '.sc': 'scala',
                    '.cs': 'csharp',
                    '.fs': 'fsharp', '.fsx': 'fsharp',
                    '.rb': 'ruby', '.rake': 'ruby',
                    '.php': 'php',
                    '.rs': 'rust',
                    '.dart': 'dart',
                    '.swift': 'swift',
                    '.m': 'objc', '.mm': 'objc',
                    '.c': 'c', '.h': 'c',
                    '.cpp': 'cpp', '.cc': 'cpp', '.cxx': 'cpp', '.hpp': 'cpp', '.hxx': 'cpp',
                    '.ex': 'elixir', '.exs': 'elixir',
                    '.erl': 'erlang', '.hrl': 'erlang',
                    '.hs': 'haskell', '.lhs': 'haskell',
                    '.lua': 'lua',
                    '.pl': 'perl', '.pm': 'perl',
                    '.r': 'r', '.R': 'r',
                    '.jl': 'julia',
                    '.clj': 'clojure', '.cljs': 'clojure', '.cljc': 'clojure',
                }

                for ext, count in ext_count.items():
                    if count >= 3 and ext in ext_to_lang:  # 至少3个同类型文件
                        detected_types.add(ext_to_lang[ext])

        except OSError:
            pass

        return detected_types

    def build_exclude_dirs(self, repo_path=None):
        """根据项目类型动态构建排除目录列表

        Args:
            repo_path: 仓库路径，用于检测项目类型。如果为 None，则返回基础排除目录

        Returns:
            set: 排除目录集合
        """
        # 始终包含基础排除目录
        exclude_dirs = BASE_EXCLUDE_DIRS.copy()
        # 默认启用智能排除，除非用户指定 --no-smart-exclude
        no_smart_exclude = getattr(self.args, 'no_smart_exclude', False)
        smart_exclude = not no_smart_exclude
        verbose = getattr(self.args, 'verbose', False)

        if repo_path and smart_exclude:
            # 检测项目类型
            detected_types = self.detect_project_types(repo_path)

            if detected_types:
                if verbose:
                    print(f"  检测到项目类型: {', '.join(sorted(detected_types))}")

                # 添加检测到的语言对应的排除目录
                for lang in detected_types:
                    if lang in LANGUAGE_EXCLUDE_DIRS:
                        exclude_dirs.update(LANGUAGE_EXCLUDE_DIRS[lang])
            else:
                # 未检测到特定类型，使用所有语言的排除目录
                if verbose:
                    print("  未检测到特定项目类型，使用完整排除列表")
                for lang_excludes in LANGUAGE_EXCLUDE_DIRS.values():
                    exclude_dirs.update(lang_excludes)
        elif not smart_exclude:
            # 不使用智能排除，使用完整排除列表
            for lang_excludes in LANGUAGE_EXCLUDE_DIRS.values():
                exclude_dirs.update(lang_excludes)

        return exclude_dirs

    def should_exclude_dir(self, dir_path):
        """检查目录是否应该被排除"""
        if self.args.all:
            return False

        dir_name = os.path.basename(dir_path)

        # 检查完全匹配
        if dir_name in self.exclude_dirs:
            return True

        # 检查模式匹配
        for pattern in self.exclude_dirs:
            if '*' in pattern or '?' in pattern:
                if fnmatch.fnmatch(dir_name, pattern):
                    return True

        # 检查正则表达式匹配
        for regex in self.exclude_dir_patterns:
            if regex.search(dir_path):
                return True

        # 检查 gitignore 规则
        if self.gitignore_matcher and self.gitignore_matcher.is_ignored(dir_path, is_dir=True):
            return True

        return False
    
    def is_code_file(self, file_path):
        """检查文件是否是代码文件"""
        file_name = os.path.basename(file_path)
        file_ext = Path(file_path).suffix.lower()

        # 排除二进制文件
        if file_ext in BINARY_EXTENSIONS:
            return False

        # 检查 gitignore 规则
        if self.gitignore_matcher and self.gitignore_matcher.is_ignored(file_path, is_dir=False):
            return False

        # 检查文件排除正则表达式
        for regex in self.exclude_file_patterns:
            if regex.search(file_path):
                return False

        # 如果指定了包含模式，则只处理匹配的文件
        # 注意：如果指定了 include_file_patterns，匹配的文件会直接被包含
        if self.include_file_patterns:
            matched = False
            for regex in self.include_file_patterns:
                if regex.search(file_path):
                    matched = True
                    break
            if not matched:
                return False
            # 如果匹配了包含模式，且没有指定语言过滤，直接返回 True
            if not self.target_languages:
                return True
            # 如果同时指定了语言过滤，需要检查语言
            lang = LANGUAGE_MAP.get(file_ext, 'other')
            return lang in self.target_languages

        # 如果指定了统计所有文件（但仍排除二进制）
        if self.args.all:
            return True

        # 排除文档文件
        if file_ext in DOC_EXTENSIONS or file_name.lower() in DOC_EXTENSIONS:
            return False

        # 如果指定了语言过滤
        if self.target_languages:
            lang = LANGUAGE_MAP.get(file_ext, 'other')
            if lang not in self.target_languages:
                return False

        # 包含代码文件
        if file_ext in CODE_EXTENSIONS:
            return True
        
        # 检查没有扩展名但可能是脚本的文件
        special_files = {
            'Makefile', 'makefile', 'GNUmakefile', 'BSDmakefile',
            'Dockerfile', 'dockerfile', 'Containerfile',
            'Jenkinsfile', 'jenkinsfile',
            'Rakefile', 'rakefile',
            'Gemfile', 'Guardfile', 'Capfile', 'Thorfile',
            'Vagrantfile', 'Berksfile', 'Cheffile',
            'Pipfile', 'SConstruct', 'SConscript',
            'BUILD', 'BUILD.bazel', 'WORKSPACE',
            'CMakeLists.txt', 'meson.build',
            'Cargo.toml', 'go.mod', 'go.sum',
            'package.json', 'tsconfig.json',
            'pom.xml', 'build.gradle', 'build.gradle.kts',
            'requirements.txt', 'setup.py', 'setup.cfg',
            'composer.json', 'phpunit.xml',
            'Podfile', 'Package.swift',
            '.gitlab-ci.yml', '.travis.yml', '.circleci/config.yml',
            'azure-pipelines.yml', 'appveyor.yml',
            '.github/workflows/*.yml', '.github/workflows/*.yaml'
        }
        
        if not file_ext and file_name in special_files:
            return True
        
        # 检查特殊模式匹配
        for pattern in special_files:
            if '*' in pattern and fnmatch.fnmatch(file_name, pattern):
                return True
        
        return False
    
    def is_binary_file(self, file_path):
        """检测文件是否为二进制文件"""
        try:
            with open(file_path, 'rb') as f:
                # 读取前8192字节来判断
                chunk = f.read(8192)
                if not chunk:
                    return False
                
                # 检测NULL字节
                if b'\x00' in chunk:
                    return True
                
                # 检测非文本字符的比例
                text_chars = bytearray({7, 8, 9, 10, 12, 13, 27} | set(range(0x20, 0x100)) - {0x7f})
                non_text = len([b for b in chunk if b not in text_chars])
                
                # 如果非文本字符超过30%，认为是二进制文件
                if non_text / len(chunk) > 0.30:
                    return True
                    
            return False
        except Exception:
            return True  # 读取出错时假定为二进制文件

    def analyze_file_content(self, file_path):
        """分析文件内容，返回详细的行数统计和复杂度信息"""
        # 先检测是否为二进制文件
        if self.is_binary_file(file_path):
            return {'total': 0, 'code': 0, 'comment': 0, 'blank': 0, 'size': 0,
                    'functions': 0, 'complexity': 0, 'max_depth': 0}

        try:
            file_size = os.path.getsize(file_path)
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                content = f.read()
                lines = content.split('\n')

            total_lines = len(lines)
            code_lines = 0
            comment_lines = 0
            blank_lines = 0

            # 复杂度分析变量
            functions = 0
            complexity = 1  # 基础复杂度为1
            max_depth = 0
            current_depth = 0

            # 获取文件扩展名以确定注释风格
            ext = Path(file_path).suffix.lower()
            lang = LANGUAGE_MAP.get(ext, 'other')

            # 复杂度分析的关键字（按语言）
            complexity_keywords = {
                'python': ['if ', 'elif ', 'for ', 'while ', 'except ', 'with ', 'and ', 'or ', 'case '],
                'javascript': ['if ', 'else if ', 'for ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
                'typescript': ['if ', 'else if ', 'for ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
                'go': ['if ', 'else if ', 'for ', 'switch ', 'case ', 'select ', '&&', '||'],
                'java': ['if ', 'else if ', 'for ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
                'cpp': ['if ', 'else if ', 'for ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
                'c': ['if ', 'else if ', 'for ', 'while ', 'case ', '&&', '||', '\\?'],
                'rust': ['if ', 'else if ', 'for ', 'while ', 'match ', '=>', '&&', '||'],
                'php': ['if ', 'elseif ', 'for ', 'foreach ', 'while ', 'catch ', 'case ', '&&', '||', '\\?'],
            }

            # 函数定义模式
            function_patterns = {
                'python': r'^\s*def\s+\w+|^\s*async\s+def\s+\w+|^\s*class\s+\w+',
                'javascript': r'function\s+\w+|^\s*\w+\s*[=:]\s*(?:async\s*)?\(|^\s*(?:async\s+)?(?:function|\w+)\s*\(',
                'typescript': r'function\s+\w+|^\s*\w+\s*[=:]\s*(?:async\s*)?\(|^\s*(?:async\s+)?(?:function|\w+)\s*\(',
                'go': r'func\s+(?:\(\w+\s+\*?\w+\)\s*)?\w+',
                'java': r'(?:public|private|protected|static|\s)+[\w<>\[\]]+\s+\w+\s*\([^)]*\)\s*(?:throws\s+[\w,\s]+)?\s*\{',
                'cpp': r'(?:\w+\s+)+\w+::\w+\s*\(|(?:\w+\s+)+\w+\s*\([^)]*\)\s*\{',
                'c': r'(?:\w+\s+)+\w+\s*\([^)]*\)\s*\{',
                'rust': r'fn\s+\w+|impl\s+\w+',
                'php': r'function\s+\w+|public\s+function|private\s+function|protected\s+function',
            }

            func_pattern = function_patterns.get(lang)
            keywords = complexity_keywords.get(lang, [])
            
            # 定义各语言的注释模式
            single_line_comment = {
                'python': '#', 'ruby': '#', 'perl': '#', 'shell': '#', 'yaml': '#',
                'javascript': '//', 'typescript': '//', 'java': '//', 'cpp': '//', 
                'c': '//', 'csharp': '//', 'go': '//', 'rust': '//', 'swift': '//',
                'kotlin': '//', 'scala': '//', 'dart': '//', 'php': '//',
                'sql': '--', 'lua': '--', 'haskell': '--', 'elm': '--',
                'assembly': ';', 'lisp': ';', 'clojure': ';', 'scheme': ';',
                'fortran': '!', 'vbnet': "'", 'matlab': '%', 'latex': '%',
                'erlang': '%', 'prolog': '%'
            }
            
            multi_line_comment = {
                'c': ('/*', '*/'), 'cpp': ('/*', '*/'), 'java': ('/*', '*/'),
                'javascript': ('/*', '*/'), 'typescript': ('/*', '*/'),
                'css': ('/*', '*/'), 'php': ('/*', '*/'), 'go': ('/*', '*/'),
                'rust': ('/*', '*/'), 'swift': ('/*', '*/'), 'kotlin': ('/*', '*/'),
                'scala': ('/*', '*/'), 'dart': ('/*', '*/'), 'sql': ('/*', '*/'),
                'html': ('<!--', '-->'), 'xml': ('<!--', '-->'),
                'python': ('"""', '"""'), 'ruby': ('=begin', '=end'),
                'lua': ('--[[', ']]'), 'haskell': ('{-', '-}')
            }
            
            # 获取当前语言的注释符号
            single_comment = single_line_comment.get(lang, '#')
            multi_comment = multi_line_comment.get(lang, None)
            
            in_multi_comment = False
            multi_start, multi_end = multi_comment if multi_comment else (None, None)
            
            for line in lines:
                stripped = line.strip()
                
                # 空行
                if not stripped:
                    blank_lines += 1
                    continue
                
                # 处理多行注释
                if multi_comment:
                    # Python 的特殊处理（docstring）
                    if lang == 'python' and '"""' in stripped:
                        if stripped.count('"""') == 2:
                            # 单行 docstring
                            comment_lines += 1
                            continue
                        else:
                            in_multi_comment = not in_multi_comment
                            comment_lines += 1
                            continue
                    
                    # 其他语言的多行注释
                    if multi_start in stripped and multi_end in stripped:
                        # 单行内的多行注释
                        comment_lines += 1
                        continue
                    elif multi_start in stripped:
                        in_multi_comment = True
                        comment_lines += 1
                        continue
                    elif multi_end in stripped:
                        in_multi_comment = False
                        comment_lines += 1
                        continue
                    elif in_multi_comment:
                        comment_lines += 1
                        continue
                
                # 单行注释
                if stripped.startswith(single_comment):
                    comment_lines += 1
                else:
                    code_lines += 1
                    # 复杂度分析（仅对代码行）
                    if func_pattern and re.search(func_pattern, line):
                        functions += 1
                    for kw in keywords:
                        if kw in line:
                            complexity += 1
                    # 嵌套深度分析（基于缩进或大括号）
                    if lang == 'python':
                        indent = len(line) - len(line.lstrip())
                        depth = indent // 4  # 假设4空格缩进
                        max_depth = max(max_depth, depth)
                    else:
                        current_depth += line.count('{') - line.count('}')
                        max_depth = max(max_depth, current_depth)

            # 应用行数过滤
            if self.args.min_lines and total_lines < self.args.min_lines:
                return {'total': 0, 'code': 0, 'comment': 0, 'blank': 0, 'size': 0,
                        'functions': 0, 'complexity': 0, 'max_depth': 0}

            if self.args.max_lines and total_lines > self.args.max_lines:
                return {'total': 0, 'code': 0, 'comment': 0, 'blank': 0, 'size': 0,
                        'functions': 0, 'complexity': 0, 'max_depth': 0}

            return {
                'total': total_lines,
                'code': code_lines,
                'comment': comment_lines,
                'blank': blank_lines,
                'size': file_size,
                'functions': functions,
                'complexity': complexity,
                'max_depth': max_depth
            }

        except Exception as e:
            if self.args.verbose:
                print(f"Error reading {file_path}: {e}")
            return {'total': 0, 'code': 0, 'comment': 0, 'blank': 0, 'size': 0,
                    'functions': 0, 'complexity': 0, 'max_depth': 0}
    
    def count_lines(self, file_path):
        """统计文件行数（向后兼容）"""
        result = self.analyze_file_content(file_path)
        return result['total']
    
    def analyze_repository(self, repo_path):
        """分析单个仓库的代码统计"""
        # 动态构建排除目录列表
        self.exclude_dirs = self.build_exclude_dirs(repo_path)
        # 添加用户指定的额外排除目录
        self.exclude_dirs.update(self.user_exclude_dirs)

        # 初始化 gitignore matcher
        if self.use_gitignore:
            self.gitignore_matcher = GitIgnoreMatcher(repo_path)
            if self.gitignore_matcher.enabled and not self.args.quiet:
                print("  ℹ️  已加载 .gitignore 规则")
        else:
            self.gitignore_matcher = None

        stats = defaultdict(lambda: {
            'files': 0, 'lines': 0, 'code_lines': 0,
            'comment_lines': 0, 'blank_lines': 0, 'size': 0
        })
        doc_stats = defaultdict(lambda: {
            'files': 0, 'lines': 0, 'size': 0
        })
        total_files = 0
        total_lines = 0
        total_code_lines = 0
        total_comment_lines = 0
        total_blank_lines = 0
        total_size = 0
        total_doc_files = 0
        total_doc_lines = 0
        total_doc_size = 0
        total_all_size = 0  # 仓库总大小
        file_details = []

        # 文件大小分布统计
        size_distribution = {
            'tiny': 0,      # < 1KB
            'small': 0,     # 1KB - 10KB
            'medium': 0,    # 10KB - 100KB
            'large': 0,     # 100KB - 1MB
            'huge': 0       # > 1MB
        }

        # 代码复杂度统计
        total_functions = 0
        total_complexity = 0
        max_depth = 0

        for root, dirs, files in os.walk(repo_path):
            # 计算当前深度
            current_depth = root.replace(repo_path, '').count(os.sep)

            # 检查深度限制
            if self.max_depth > 0 and current_depth >= self.max_depth:
                dirs[:] = []  # 不再进入子目录
                continue

            # 过滤掉需要排除的目录
            dirs[:] = [d for d in dirs if not self.should_exclude_dir(os.path.join(root, d))]

            for file in files:
                file_path = os.path.join(root, file)
                
                # 统计仓库总大小（所有文件）
                if os.path.isfile(file_path):
                    try:
                        total_all_size += os.path.getsize(file_path)
                    except:
                        pass
                
                # 判断是否是文档文件
                file_ext = Path(file_path).suffix.lower()
                file_name = os.path.basename(file_path).lower()

                # 检查 gitignore 规则（用于文档文件）
                if self.gitignore_matcher and self.gitignore_matcher.is_ignored(file_path, is_dir=False):
                    continue

                # 如果用户指定了 include_file_patterns，优先检查是否匹配
                # 匹配的文件应该作为代码文件处理，而不是文档文件
                is_explicit_include = False
                if self.include_file_patterns:
                    for regex in self.include_file_patterns:
                        if regex.search(file_path):
                            is_explicit_include = True
                            break

                # 如果不是显式包含的文件，且是文档扩展名，则作为文档统计
                if not is_explicit_include and (file_ext in DOC_EXTENSIONS or file_name in DOC_EXTENSIONS):
                    # 统计文档文件
                    try:
                        file_size = os.path.getsize(file_path)
                        with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                            lines = len(f.readlines())

                        ext = file_ext or file_name
                        doc_stats[ext]['files'] += 1
                        doc_stats[ext]['lines'] += lines
                        doc_stats[ext]['size'] += file_size

                        total_doc_files += 1
                        total_doc_lines += lines
                        total_doc_size += file_size
                    except:
                        pass

                elif self.is_code_file(file_path):
                    # 统计代码文件
                    file_stats = self.analyze_file_content(file_path)
                    if file_stats['total'] > 0:
                        ext = Path(file_path).suffix.lower() or 'no_extension'
                        stats[ext]['files'] += 1
                        stats[ext]['lines'] += file_stats['total']
                        stats[ext]['code_lines'] += file_stats['code']
                        stats[ext]['comment_lines'] += file_stats['comment']
                        stats[ext]['blank_lines'] += file_stats['blank']
                        stats[ext]['size'] += file_stats['size']
                        
                        total_files += 1
                        total_lines += file_stats['total']
                        total_code_lines += file_stats['code']
                        total_comment_lines += file_stats['comment']
                        total_blank_lines += file_stats['blank']
                        total_size += file_stats['size']

                        # 更新文件大小分布
                        fsize = file_stats['size']
                        if fsize < 1024:
                            size_distribution['tiny'] += 1
                        elif fsize < 10 * 1024:
                            size_distribution['small'] += 1
                        elif fsize < 100 * 1024:
                            size_distribution['medium'] += 1
                        elif fsize < 1024 * 1024:
                            size_distribution['large'] += 1
                        else:
                            size_distribution['huge'] += 1

                        # 更新复杂度统计
                        total_functions += file_stats.get('functions', 0)
                        total_complexity += file_stats.get('complexity', 0)
                        max_depth = max(max_depth, file_stats.get('max_depth', 0))

                        if self.args.verbose:
                            relative_path = os.path.relpath(file_path, repo_path)
                            file_details.append({
                                'path': relative_path,
                                'lines': file_stats['total'],
                                'code_lines': file_stats['code'],
                                'comment_lines': file_stats['comment'],
                                'blank_lines': file_stats['blank'],
                                'size': file_stats['size'],
                                'extension': ext
                            })
        
        # 计算平均复杂度
        avg_complexity = total_complexity / total_files if total_files > 0 else 0
        avg_func_lines = total_code_lines / total_functions if total_functions > 0 else 0

        return {
            'total_files': total_files,
            'total_lines': total_lines,
            'total_code_lines': total_code_lines,
            'total_comment_lines': total_comment_lines,
            'total_blank_lines': total_blank_lines,
            'total_size': total_size,
            'doc_files': total_doc_files,
            'doc_lines': total_doc_lines,
            'doc_size': total_doc_size,
            'total_all_size': total_all_size,  # 仓库总大小
            'by_extension': dict(stats),
            'doc_details': dict(doc_stats),
            'file_details': file_details if self.args.verbose else [],
            'size_distribution': size_distribution,
            'complexity': {
                'functions': total_functions,
                'total_complexity': total_complexity,
                'avg_complexity': round(avg_complexity, 2),
                'max_depth': max_depth,
                'avg_func_lines': round(avg_func_lines, 1)
            }
        }
    
    def get_git_info(self, repo_path):
        """获取 Git 仓库信息"""
        git_info = {
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
                git_info['contributors'] = len(result.stdout.strip().split('\n'))

            # 获取远程仓库 URL
            result = subprocess.run(
                ['git', '-C', repo_path, 'remote', 'get-url', 'origin'],
                capture_output=True, text=True, timeout=5
            )
            if result.returncode == 0:
                git_info['remote_url'] = result.stdout.strip()

        except subprocess.TimeoutExpired:
            if self.args.verbose:
                print(f"警告：获取 {repo_path} 的 Git 信息超时")
        except Exception as e:
            if self.args.verbose:
                print(f"警告：获取 {repo_path} 的 Git 信息失败: {e}")

        return git_info

    def detect_language(self, repo_stats):
        """根据文件扩展名检测主要编程语言"""
        # 统计各语言的代码行数
        language_lines = defaultdict(int)
        for ext, data in repo_stats['by_extension'].items():
            lang = LANGUAGE_MAP.get(ext, 'other')
            language_lines[lang] += data['lines']
        
        # 返回代码行数最多的语言
        if language_lines:
            return max(language_lines.items(), key=lambda x: x[1])[0]
        return 'unknown'
    
    def print_progress(self, message):
        """打印进度信息"""
        if self.args.verbose:
            self.progress_count += 1
            print(f"[{self.progress_count}/{self.total_repos}] {message}")
    
    def analyze_all_repos(self):
        """分析所有仓库"""
        all_repos = []
        repo_paths = []
        
        # 收集所有仓库路径
        # 如果指定了 --dirs 参数，统计指定的目录
        if self.args.dirs:
            for dir_name in self.args.dirs:
                # 支持绝对路径和相对路径
                if os.path.isabs(dir_name):
                    dir_path = dir_name
                else:
                    dir_path = os.path.join(self.current_dir, dir_name)
                
                if os.path.isdir(dir_path):
                    repo_name = os.path.basename(dir_path)
                    repo_paths.append((repo_name, dir_path))
                else:
                    print(f"警告：目录不存在或不是目录: {dir_path}")
        else:
            # 原有逻辑：扫描当前目录下的所有仓库
            items = os.listdir(self.current_dir)
            
            # 如果指定了特定仓库
            if self.args.repo:
                target_repos = set(repo.strip() for repo in self.args.repo.split(','))
                items = [item for item in items if item in target_repos]
            
            for item in sorted(items):
                item_path = os.path.join(self.current_dir, item)
                
                # 跳过非目录和以.开头的隐藏目录
                if not os.path.isdir(item_path) or item.startswith('.'):
                    continue
                
                # 跳过非仓库目录（仅在未指定 dirs 时检查）
                if not os.path.exists(os.path.join(item_path, '.git')):
                    continue
                
                repo_paths.append((item, item_path))
        
        self.total_repos = len(repo_paths)
        
        # 使用多线程并行处理
        if self.args.parallel and len(repo_paths) > 1:
            with concurrent.futures.ThreadPoolExecutor(max_workers=os.cpu_count()) as executor:
                future_to_repo = {
                    executor.submit(self.analyze_repository, path): (name, path) 
                    for name, path in repo_paths
                }
                
                for future in concurrent.futures.as_completed(future_to_repo):
                    name, path = future_to_repo[future]
                    self.print_progress(f"分析仓库: {name}")
                    
                    try:
                        stats = future.result()
                        if stats['total_files'] > 0:
                            main_language = self.detect_language(stats)
                            repo_info = {
                                'name': name,
                                'language': main_language,
                                'files': stats['total_files'],
                                'lines': stats['total_lines'],
                                'code_lines': stats['total_code_lines'],
                                'comment_lines': stats['total_comment_lines'],
                                'blank_lines': stats['total_blank_lines'],
                                'size': stats['total_size'],
                                'doc_files': stats.get('doc_files', 0),
                                'doc_lines': stats.get('doc_lines', 0),
                                'doc_size': stats.get('doc_size', 0),
                                'total_size': stats.get('total_all_size', stats['total_size']),
                                'details': stats['by_extension'],
                                'doc_details': stats.get('doc_details', {}),
                                'file_details': stats.get('file_details', []),
                                'size_distribution': stats.get('size_distribution', {}),
                                'complexity': stats.get('complexity', {})
                            }
                            # 添加 Git 信息
                            if self.git_info:
                                repo_info['git'] = self.get_git_info(path)
                            all_repos.append(repo_info)
                    except Exception as e:
                        print(f"Error analyzing {name}: {e}")
        else:
            # 串行处理
            for name, path in repo_paths:
                self.print_progress(f"分析仓库: {name}")
                stats = self.analyze_repository(path)
                
                if stats['total_files'] > 0:
                    main_language = self.detect_language(stats)
                    repo_info = {
                        'name': name,
                        'path': path,
                        'language': main_language,
                        'files': stats['total_files'],
                        'lines': stats['total_lines'],
                        'code_lines': stats['total_code_lines'],
                        'comment_lines': stats['total_comment_lines'],
                        'blank_lines': stats['total_blank_lines'],
                        'size': stats['total_size'],
                        'doc_files': stats.get('doc_files', 0),
                        'doc_lines': stats.get('doc_lines', 0),
                        'doc_size': stats.get('doc_size', 0),
                        'total_size': stats.get('total_all_size', stats['total_size']),
                        'details': stats['by_extension'],
                        'doc_details': stats.get('doc_details', {}),
                        'file_details': stats.get('file_details', []),
                        'size_distribution': stats.get('size_distribution', {}),
                        'complexity': stats.get('complexity', {})
                    }
                    # 添加 Git 信息
                    if self.git_info:
                        repo_info['git'] = self.get_git_info(path)
                    all_repos.append(repo_info)
        
        return all_repos
    
    def generate_summary(self, all_repos):
        """生成统计摘要"""
        total_all_files = sum(repo['files'] for repo in all_repos)
        total_all_lines = sum(repo['lines'] for repo in all_repos)
        total_all_code_lines = sum(repo.get('code_lines', repo['lines']) for repo in all_repos)
        total_all_comment_lines = sum(repo.get('comment_lines', 0) for repo in all_repos)
        total_all_blank_lines = sum(repo.get('blank_lines', 0) for repo in all_repos)
        total_all_size = sum(repo.get('size', 0) for repo in all_repos)
        
        # 文档统计
        total_all_doc_files = sum(repo.get('doc_files', 0) for repo in all_repos)
        total_all_doc_lines = sum(repo.get('doc_lines', 0) for repo in all_repos)
        total_all_doc_size = sum(repo.get('doc_size', 0) for repo in all_repos)
        total_all_total_size = sum(repo.get('total_size', repo.get('size', 0)) for repo in all_repos)

        # 汇总代码详情（按扩展名）
        code_details_summary = defaultdict(lambda: {'files': 0, 'lines': 0, 'code_lines': 0, 'comment_lines': 0, 'blank_lines': 0, 'size': 0})
        for repo in all_repos:
            for ext, details in repo.get('details', {}).items():
                code_details_summary[ext]['files'] += details.get('files', 0)
                code_details_summary[ext]['lines'] += details.get('lines', 0)
                code_details_summary[ext]['code_lines'] += details.get('code_lines', 0)
                code_details_summary[ext]['comment_lines'] += details.get('comment_lines', 0)
                code_details_summary[ext]['blank_lines'] += details.get('blank_lines', 0)
                code_details_summary[ext]['size'] += details.get('size', 0)

        # 汇总文档详情（按扩展名）
        doc_details_summary = defaultdict(lambda: {'files': 0, 'lines': 0, 'size': 0})
        for repo in all_repos:
            for ext, details in repo.get('doc_details', {}).items():
                doc_details_summary[ext]['files'] += details.get('files', 0)
                doc_details_summary[ext]['lines'] += details.get('lines', 0)
                doc_details_summary[ext]['size'] += details.get('size', 0)

        # 汇总文件大小分布
        size_distribution_summary = {'tiny': 0, 'small': 0, 'medium': 0, 'large': 0, 'huge': 0}
        for repo in all_repos:
            repo_dist = repo.get('size_distribution', {})
            for key in size_distribution_summary:
                size_distribution_summary[key] += repo_dist.get(key, 0)

        # 汇总复杂度数据
        total_functions = sum(repo.get('complexity', {}).get('functions', 0) for repo in all_repos)
        total_complexity = sum(repo.get('complexity', {}).get('total_complexity', 0) for repo in all_repos)
        max_depth = max((repo.get('complexity', {}).get('max_depth', 0) for repo in all_repos), default=0)
        avg_complexity = total_complexity / total_all_files if total_all_files > 0 else 0
        avg_func_lines = total_all_code_lines / total_functions if total_functions > 0 else 0

        complexity_summary = {
            'functions': total_functions,
            'total_complexity': total_complexity,
            'avg_complexity': round(avg_complexity, 2),
            'max_depth': max_depth,
            'avg_func_lines': round(avg_func_lines, 1)
        }

        language_summary = defaultdict(lambda: {
            'repos': 0, 'files': 0, 'lines': 0, 
            'code_lines': 0, 'comment_lines': 0, 
            'blank_lines': 0, 'size': 0, 'doc_files': 0,
            'doc_lines': 0, 'doc_size': 0
        })
        
        for repo in all_repos:
            lang = repo['language']
            language_summary[lang]['repos'] += 1
            language_summary[lang]['files'] += repo['files']
            language_summary[lang]['lines'] += repo['lines']
            language_summary[lang]['code_lines'] += repo.get('code_lines', repo['lines'])
            language_summary[lang]['comment_lines'] += repo.get('comment_lines', 0)
            language_summary[lang]['blank_lines'] += repo.get('blank_lines', 0)
            language_summary[lang]['size'] += repo.get('size', 0)
            language_summary[lang]['doc_files'] += repo.get('doc_files', 0)
            language_summary[lang]['doc_lines'] += repo.get('doc_lines', 0)
            language_summary[lang]['doc_size'] += repo.get('doc_size', 0)
        
        return {
            'total_repos': len(all_repos),
            'total_files': total_all_files,
            'total_lines': total_all_lines,
            'total_code_lines': total_all_code_lines,
            'total_comment_lines': total_all_comment_lines,
            'total_blank_lines': total_all_blank_lines,
            'total_size': total_all_size,
            'total_doc_files': total_all_doc_files,
            'total_doc_lines': total_all_doc_lines,
            'total_doc_size': total_all_doc_size,
            'total_all_size': total_all_total_size,
            'by_language': dict(language_summary),
            'code_details': dict(code_details_summary),
            'doc_details': dict(doc_details_summary),
            'size_distribution': size_distribution_summary,
            'complexity': complexity_summary
        }
    
    def output_json(self, all_repos, summary):
        """输出JSON格式"""
        result = {
            'generated_at': datetime.now().isoformat(),
            'summary': summary,
            'repositories': all_repos
        }
        
        output_file = self.args.output_file or 'code_statistics.json'
        with open(output_file, 'w', encoding='utf-8') as f:
            json.dump(result, f, ensure_ascii=False, indent=2)
        
        if not self.args.summary:
            print(f"\n详细统计结果已保存到: {output_file}")
    
    def output_csv(self, all_repos, summary):
        """输出CSV格式"""
        output_file = self.args.output_file or 'code_statistics.csv'
        
        with open(output_file, 'w', newline='', encoding='utf-8') as f:
            writer = csv.writer(f)
            writer.writerow(['Repository', 'Language', 'Code_Files', 'Code_Lines', 'Doc_Files', 'Doc_Lines', 'Code_Size_MB', 'Total_Size_MB'])
            
            for repo in all_repos:
                code_size_mb = round(repo.get('size', 0) / (1024 * 1024), 2)
                total_size_mb = round(repo.get('total_size', 0) / (1024 * 1024), 2)
                writer.writerow([
                    repo['name'], 
                    repo['language'], 
                    repo['files'], 
                    repo['lines'],
                    repo.get('doc_files', 0),
                    repo.get('doc_lines', 0),
                    code_size_mb,
                    total_size_mb
                ])
            
            writer.writerow([])
            writer.writerow(['Summary'])
            writer.writerow(['Total Repos', summary['total_repos']])
            writer.writerow(['Total Code Files', summary['total_files']])
            writer.writerow(['Total Code Lines', summary['total_lines']])
            writer.writerow(['Total Doc Files', summary.get('total_doc_files', 0)])
            writer.writerow(['Total Doc Lines', summary.get('total_doc_lines', 0)])
            writer.writerow(['Total Code Size (MB)', round(summary.get('total_size', 0) / (1024 * 1024), 2)])
            writer.writerow(['Total Repository Size (MB)', round(summary.get('total_all_size', 0) / (1024 * 1024), 2)])
        
        if not self.args.summary:
            print(f"\n统计结果已保存到: {output_file}")
    
    def output_markdown(self, all_repos, summary):
        """输出Markdown格式"""
        output_file = self.args.output_file or 'code_statistics.md'
        
        with open(output_file, 'w', encoding='utf-8') as f:
            f.write("# 代码统计报告\n\n")
            f.write(f"> 📊 生成时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}\n")
            f.write(f"> 📁 扫描目录: `{self.current_dir}`\n\n")
            
            # 执行摘要
            f.write("## 📋 执行摘要\n\n")
            f.write("### 🎯 统计概览\n\n")
            f.write("| 指标 | 数值 |\n")
            f.write("|------|------|\n")
            f.write(f"| 🗂️ **仓库总数** | {summary['total_repos']} |\n")
            f.write(f"| 📄 **代码文件总数** | {summary['total_files']:,} |\n")
            f.write(f"| 📝 **代码总行数** | {summary['total_lines']:,} |\n")
            f.write(f"| 📋 **文档文件总数** | {summary.get('total_doc_files', 0):,} |\n")
            f.write(f"| 📑 **文档总行数** | {summary.get('total_doc_lines', 0):,} |\n")
            f.write(f"| 💾 **代码文件大小** | {self.format_size(summary.get('total_size', 0))} |\n")
            f.write(f"| 📁 **仓库总大小** | {self.format_size(summary.get('total_all_size', 0))} |\n")
            f.write(f"| 📊 **平均每仓库代码行数** | {summary['total_lines'] // summary['total_repos'] if summary['total_repos'] > 0 else 0:,} |\n")
            f.write(f"| 📈 **平均每文件代码行数** | {summary['total_lines'] // summary['total_files'] if summary['total_files'] > 0 else 0} |\n\n")
            
            # 语言分布饼图（使用emoji模拟）
            f.write("### 🌐 语言分布\n\n")
            sorted_languages = sorted(summary['by_language'].items(), 
                                    key=lambda x: x[1]['lines'], reverse=True)
            
            # 使用进度条可视化
            max_lines = sorted_languages[0][1]['lines'] if sorted_languages else 0
            for lang, data in sorted_languages[:10]:  # 只显示前10种语言
                percentage = (data['lines'] / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
                bar_length = int((data['lines'] / max_lines) * 40) if max_lines > 0 else 0
                bar = '█' * bar_length + '░' * (40 - bar_length)
                f.write(f"**{lang:>15}** |{bar}| {percentage:>5.1f}% ({data['lines']:,} 行)\n")
            
            if len(sorted_languages) > 10:
                f.write(f"\n*... 以及其他 {len(sorted_languages) - 10} 种语言*\n")
            
            # 按语言详细统计表
            f.write("\n## 📊 按语言统计\n\n")
            f.write("| # | 语言 | 仓库数 | 文件数 | 代码行数 | 占比 | 平均行/文件 |\n")
            f.write("|---|------|--------|--------|----------|------|-------------|\n")
            
            for idx, (lang, data) in enumerate(sorted_languages, 1):
                percentage = (data['lines'] / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
                avg_lines = data['lines'] // data['files'] if data['files'] > 0 else 0
                lang_emoji = self.get_language_emoji(lang)
                f.write(f"| {idx} | {lang_emoji} {lang} | {data['repos']} | {data['files']:,} | {data['lines']:,} | {percentage:.1f}% | {avg_lines} |\n")
            
            # 仓库排行榜
            f.write("\n## 🏆 仓库排行榜\n\n")
            
            # 按代码行数排序
            sorted_by_lines = sorted(all_repos, key=lambda x: x['lines'], reverse=True)
            f.write("### 📈 按代码行数 TOP 10\n\n")
            f.write("| 排名 | 仓库名 | 主要语言 | 代码文件 | 代码行数 | 文档文件 | 文档行数 | 占比 |\n")
            f.write("|------|--------|----------|--------|----------|--------|----------|------|\n")
            
            for i, repo in enumerate(sorted_by_lines[:10], 1):
                percentage = (repo['lines'] / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
                medal = "🥇" if i == 1 else "🥈" if i == 2 else "🥉" if i == 3 else f"{i}"
                lang_emoji = self.get_language_emoji(repo['language'])
                f.write(f"| {medal} | **{repo['name']}** | {lang_emoji} {repo['language']} | {repo['files']:,} | {repo['lines']:,} | {repo.get('doc_files', 0):,} | {repo.get('doc_lines', 0):,} | {percentage:.1f}% |\n")
            
            # 仓库详细列表
            f.write("\n## 📁 仓库详细信息\n\n")
            
            # 排序
            sort_key = self.args.sort
            if sort_key == 'lines':
                sorted_repos = sorted(all_repos, key=lambda x: x['lines'], reverse=True)
            elif sort_key == 'files':
                sorted_repos = sorted(all_repos, key=lambda x: x['files'], reverse=True)
            else:  # name
                sorted_repos = sorted(all_repos, key=lambda x: x['name'])
            
            # 应用top限制
            if self.args.top:
                sorted_repos = sorted_repos[:self.args.top]
            
            for repo in sorted_repos:
                lang_emoji = self.get_language_emoji(repo['language'])
                f.write(f"### 📦 {repo['name']}\n\n")
                f.write(f"- **主要语言**: {lang_emoji} {repo['language']}\n")
                f.write(f"- **代码文件数**: {repo['files']:,}\n")
                f.write(f"- **代码行数**: {repo['lines']:,}\n")
                
                # 添加代码组成分析
                total_lines = repo['lines']
                code_lines = repo.get('code_lines', 0)
                comment_lines = repo.get('comment_lines', 0)
                blank_lines = repo.get('blank_lines', 0)
                
                if total_lines > 0:
                    comment_rate = (comment_lines / total_lines) * 100
                    blank_rate = (blank_lines / total_lines) * 100
                    code_rate = (code_lines / total_lines) * 100
                    f.write(f"  - 纯代码行: {code_lines:,} ({code_rate:.1f}%)\n")
                    f.write(f"  - 注释行: {comment_lines:,} ({comment_rate:.1f}%)\n")
                    f.write(f"  - 空行: {blank_lines:,} ({blank_rate:.1f}%)\n")
                
                f.write(f"- **文档文件数**: {repo.get('doc_files', 0):,}\n")
                f.write(f"- **文档行数**: {repo.get('doc_lines', 0):,}\n")
                f.write(f"- **代码文件大小**: {self.format_size(repo.get('size', 0))}\n")
                f.write(f"- **仓库总大小**: {self.format_size(repo.get('total_size', 0))}\n")
                
                # 显示文件类型分布
                if repo.get('details'):
                    f.write("- **文件类型分布**:\n")
                    sorted_exts = sorted(repo['details'].items(), 
                                       key=lambda x: x[1]['lines'], reverse=True)[:5]
                    for ext, data in sorted_exts:
                        f.write(f"  - `{ext}`: {data['files']} 个文件, {data['lines']:,} 行\n")
                f.write("\n")
            
            # 统计信息页脚
            f.write("---\n\n")
            f.write("*使用 [code_statistics.py](https://github.com/yourusername/code-statistics) 生成*\n")
        
        if not self.args.summary:
            print(f"\n统计结果已保存到: {output_file}")
    
    def output_html(self, all_repos, summary):
        """输出HTML格式 - 现代简洁风格"""
        output_file = self.args.output_file or 'code_statistics.html'

        # 准备数据
        gen_time = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        code_details = summary.get('code_details', {})
        doc_details = summary.get('doc_details', {})
        # 类型分布显示前10个，语言和仓库显示全部
        sorted_code_types = sorted(code_details.items(), key=lambda x: x[1]['lines'], reverse=True)[:10]
        sorted_doc_types = sorted(doc_details.items(), key=lambda x: x[1]['lines'], reverse=True)[:10]
        sorted_languages = sorted(summary['by_language'].items(), key=lambda x: x[1]['lines'], reverse=True)
        sorted_repos = sorted(all_repos, key=lambda x: x['lines'], reverse=True)

        total_lines = summary['total_lines']
        total_files = summary['total_files']
        code_rate = (summary.get('total_code_lines', 0) / total_lines * 100) if total_lines > 0 else 0
        comment_rate = (summary.get('total_comment_lines', 0) / total_lines * 100) if total_lines > 0 else 0
        blank_rate = (summary.get('total_blank_lines', 0) / total_lines * 100) if total_lines > 0 else 0

        # 文件大小分布数据
        size_dist = summary.get('size_distribution', {})
        size_labels = {
            'tiny': '< 1KB',
            'small': '1-10KB',
            'medium': '10-100KB',
            'large': '100KB-1MB',
            'huge': '> 1MB'
        }

        # 复杂度数据
        complexity = summary.get('complexity', {})

        # 生成代码类型分布 HTML
        code_types_html = ''
        for ext, details in sorted_code_types:
            pct = (details['lines'] / total_lines * 100) if total_lines > 0 else 0
            code_types_html += f'''<div class="type-item">
                    <span class="type-ext">{ext}</span>
                    <div class="type-bar"><div class="type-bar-fill" style="width:{min(pct*2,100):.1f}%"></div></div>
                    <span class="type-stats"><strong>{details['lines']:,}</strong> 行 · {details['files']} 文件</span>
                </div>'''

        # 生成文档类型分布 HTML
        doc_types_html = ''
        total_doc_lines = summary.get('total_doc_lines', 0)
        if sorted_doc_types:
            for ext, details in sorted_doc_types:
                pct = (details['lines'] / total_doc_lines * 100) if total_doc_lines > 0 else 0
                doc_types_html += f'''<div class="type-item">
                    <span class="type-ext">{ext}</span>
                    <div class="type-bar"><div class="type-bar-fill doc" style="width:{min(pct,100):.1f}%"></div></div>
                    <span class="type-stats"><strong>{details['lines']:,}</strong> 行 · {details['files']} 文件</span>
                </div>'''
        else:
            doc_types_html = '<div style="color:#94a3b8;text-align:center;padding:32px;">暂无文档文件</div>'

        # 生成语言统计表格行
        lang_table_rows = ''
        for idx, (lang, data) in enumerate(sorted_languages, 1):
            pct = (data['lines'] / total_lines * 100) if total_lines > 0 else 0
            c_rate = (data.get('comment_lines', 0) / data['lines'] * 100) if data['lines'] > 0 else 0
            emoji = self.get_language_emoji(lang)
            lang_table_rows += f'''
                        <tr>
                            <td class="rank">{idx}</td>
                            <td><span class="lang-badge">{emoji} {lang}</span></td>
                            <td style="text-align:right" class="number">{data['repos']}</td>
                            <td style="text-align:right" class="number">{data['files']:,}</td>
                            <td style="text-align:right" class="number">{data['lines']:,}</td>
                            <td style="text-align:right">{c_rate:.1f}%</td>
                            <td style="text-align:right">{pct:.1f}%</td>
                        </tr>'''

        # 生成仓库排行表格行
        repo_table_rows = ''
        medals = ['🥇', '🥈', '🥉']
        for idx, repo in enumerate(sorted_repos, 1):
            c_rate = (repo.get('comment_lines', 0) / repo['lines'] * 100) if repo['lines'] > 0 else 0
            emoji = self.get_language_emoji(repo['language'])
            medal = medals[idx-1] if idx <= 3 else str(idx)
            repo_table_rows += f'''
                        <tr>
                            <td class="rank">{medal}</td>
                            <td><strong>{repo['name']}</strong></td>
                            <td><span class="lang-badge">{emoji} {repo['language']}</span></td>
                            <td style="text-align:right" class="number">{repo['files']:,}</td>
                            <td style="text-align:right" class="number">{repo['lines']:,}</td>
                            <td style="text-align:right" class="number">{repo.get('doc_lines', 0):,}</td>
                            <td style="text-align:right">{c_rate:.1f}%</td>
                            <td style="text-align:right">{self.format_size(repo.get('size', 0))}</td>
                        </tr>'''

        # Git 信息区块
        git_section = ''
        repos_with_git = [r for r in all_repos if r.get('git')]
        if repos_with_git:
            git_items = ''
            for repo in repos_with_git:
                git = repo.get('git', {})
                git_items += f'''
                    <div class="git-item">
                        <div class="name">{repo['name']}</div>
                        <div class="git-row"><span class="key">分支</span><span class="val">{git.get('branch', 'N/A')}</span></div>
                        <div class="git-row"><span class="key">最后提交</span><span class="val">{git.get('last_commit_date', 'N/A')[:10] if git.get('last_commit_date') else 'N/A'}</span></div>
                        <div class="git-row"><span class="key">提交者</span><span class="val">{git.get('last_commit_author', 'N/A')}</span></div>
                        <div class="git-row"><span class="key">提交数</span><span class="val" style="color:#3b82f6">{git.get('total_commits', 'N/A')}</span></div>
                        <div class="git-row"><span class="key">贡献者</span><span class="val" style="color:#22c55e">{git.get('contributors', 'N/A')}</span></div>
                    </div>'''
            git_section = f'''
        <div class="card">
            <div class="card-title">Git 仓库信息</div>
            <div class="git-grid">{git_items}</div>
        </div>'''

        # 图表数据
        lang_labels = json.dumps([lang for lang, _ in sorted_languages])
        lang_data = json.dumps([data['lines'] for _, data in sorted_languages])
        repo_labels = json.dumps([repo['name'] for repo in sorted_repos])
        repo_data = json.dumps([repo['lines'] for repo in sorted_repos])

        # HTML 模板 - 白色简洁专业风格
        html = f'''<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>代码统计报告</title>
    <script src="https://cdn.jsdelivr.net/npm/chart.js@4.4.1/dist/chart.umd.min.js"></script>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
            background: #f8fafc;
            color: #1e293b;
            line-height: 1.5;
            -webkit-font-smoothing: antialiased;
        }}
        .container {{ max-width: 1200px; margin: 0 auto; padding: 40px 24px; }}

        /* 头部 */
        .header {{ margin-bottom: 48px; }}
        .header h1 {{ font-size: 28px; font-weight: 600; color: #0f172a; margin-bottom: 8px; }}
        .header .meta {{ color: #64748b; font-size: 14px; }}
        .header .meta span {{ margin-right: 16px; }}

        /* 概览卡片 */
        .overview {{ display: grid; grid-template-columns: repeat(6, 1fr); gap: 16px; margin-bottom: 32px; }}
        .overview-card {{
            background: #fff;
            border: 1px solid #e2e8f0;
            border-radius: 12px;
            padding: 20px;
        }}
        .overview-card .number {{ font-size: 28px; font-weight: 700; color: #0f172a; font-variant-numeric: tabular-nums; }}
        .overview-card .label {{ font-size: 13px; color: #64748b; margin-top: 4px; }}

        /* 区块 */
        .card {{
            background: #fff;
            border: 1px solid #e2e8f0;
            border-radius: 12px;
            padding: 24px;
            margin-bottom: 24px;
        }}
        .card-title {{
            font-size: 15px;
            font-weight: 600;
            color: #0f172a;
            margin-bottom: 20px;
            display: flex;
            align-items: center;
            gap: 8px;
        }}
        .card-title::before {{
            content: '';
            width: 4px;
            height: 16px;
            background: #3b82f6;
            border-radius: 2px;
        }}

        /* 双栏 */
        .grid-2 {{ display: grid; grid-template-columns: 1fr 1fr; gap: 24px; }}

        /* 指标条 */
        .metric-bars {{ display: flex; gap: 24px; }}
        .metric-bar {{ flex: 1; text-align: center; }}
        .metric-bar .value {{ font-size: 32px; font-weight: 700; }}
        .metric-bar .label {{ font-size: 12px; color: #64748b; margin-top: 4px; }}
        .metric-bar .bar {{ height: 4px; background: #e2e8f0; border-radius: 2px; margin-top: 12px; overflow: hidden; }}
        .metric-bar .bar-fill {{ height: 100%; border-radius: 2px; }}
        .blue {{ color: #3b82f6; }}
        .green {{ color: #22c55e; }}
        .purple {{ color: #8b5cf6; }}
        .bar-fill.blue {{ background: #3b82f6; }}
        .bar-fill.green {{ background: #22c55e; }}
        .bar-fill.purple {{ background: #8b5cf6; }}

        /* 文件类型列表 */
        .type-list {{ }}
        .type-item {{
            display: flex;
            align-items: center;
            padding: 12px 0;
            border-bottom: 1px solid #f1f5f9;
        }}
        .type-item:last-child {{ border-bottom: none; }}
        .type-ext {{
            font-family: 'SF Mono', Monaco, 'Consolas', monospace;
            font-size: 13px;
            color: #0f172a;
            background: #f1f5f9;
            padding: 4px 10px;
            border-radius: 6px;
            min-width: 70px;
            text-align: center;
        }}
        .type-bar {{
            flex: 1;
            height: 8px;
            background: #f1f5f9;
            border-radius: 4px;
            margin: 0 16px;
            overflow: hidden;
        }}
        .type-bar-fill {{
            height: 100%;
            background: linear-gradient(90deg, #3b82f6, #60a5fa);
            border-radius: 4px;
        }}
        .type-bar-fill.doc {{
            background: linear-gradient(90deg, #8b5cf6, #a78bfa);
        }}
        .type-stats {{
            font-size: 13px;
            color: #64748b;
            min-width: 140px;
            text-align: right;
        }}
        .type-stats strong {{ color: #0f172a; }}

        /* 表格 */
        table {{ width: 100%; border-collapse: collapse; }}
        th {{
            text-align: left;
            padding: 12px 16px;
            font-size: 11px;
            font-weight: 600;
            color: #64748b;
            text-transform: uppercase;
            letter-spacing: 0.5px;
            border-bottom: 1px solid #e2e8f0;
            background: #f8fafc;
        }}
        td {{
            padding: 14px 16px;
            font-size: 14px;
            border-bottom: 1px solid #f1f5f9;
        }}
        tr:hover td {{ background: #f8fafc; }}
        .text-right {{ text-align: right; }}
        .mono {{ font-family: 'SF Mono', Monaco, monospace; font-variant-numeric: tabular-nums; }}
        .rank {{ font-weight: 700; color: #f59e0b; font-size: 16px; }}
        .lang-tag {{
            display: inline-flex;
            align-items: center;
            gap: 6px;
            font-size: 13px;
            color: #475569;
        }}

        /* 图表 */
        .chart-wrap {{ height: 260px; position: relative; }}

        /* Git 卡片 */
        .git-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 16px; }}
        .git-item {{
            background: #f8fafc;
            border-radius: 8px;
            padding: 16px;
        }}
        .git-item .name {{ font-weight: 600; color: #0f172a; margin-bottom: 12px; }}
        .git-row {{ display: flex; justify-content: space-between; font-size: 13px; padding: 4px 0; }}
        .git-row .key {{ color: #64748b; }}
        .git-row .val {{ color: #0f172a; }}

        /* 页脚 */
        .footer {{
            text-align: center;
            padding: 32px 0;
            font-size: 13px;
            color: #94a3b8;
            margin-top: 24px;
        }}

        /* 响应式 */
        @media (max-width: 1024px) {{
            .overview {{ grid-template-columns: repeat(3, 1fr); }}
        }}
        @media (max-width: 768px) {{
            .container {{ padding: 24px 16px; }}
            .overview {{ grid-template-columns: repeat(2, 1fr); }}
            .grid-2 {{ grid-template-columns: 1fr; }}
            .metric-bars {{ flex-direction: column; gap: 16px; }}
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <h1>代码统计报告</h1>
            <div class="meta">
                <span>📅 {gen_time}</span>
                <span>📁 {self.current_dir}</span>
            </div>
        </div>

        <div class="overview">
            <div class="overview-card"><div class="number">{summary['total_repos']}</div><div class="label">仓库</div></div>
            <div class="overview-card"><div class="number">{summary['total_files']:,}</div><div class="label">代码文件</div></div>
            <div class="overview-card"><div class="number">{summary['total_lines']:,}</div><div class="label">代码行</div></div>
            <div class="overview-card"><div class="number">{summary.get('total_doc_files', 0)}</div><div class="label">文档文件</div></div>
            <div class="overview-card"><div class="number">{summary.get('total_doc_lines', 0):,}</div><div class="label">文档行</div></div>
            <div class="overview-card"><div class="number">{self.format_size(summary.get('total_all_size', 0))}</div><div class="label">总大小</div></div>
        </div>

        <div class="card">
            <div class="card-title">代码组成</div>
            <div class="metric-bars">
                <div class="metric-bar">
                    <div class="value blue">{code_rate:.1f}%</div>
                    <div class="label">代码行 · {summary.get('total_code_lines', 0):,}</div>
                    <div class="bar"><div class="bar-fill blue" style="width:{code_rate:.1f}%"></div></div>
                </div>
                <div class="metric-bar">
                    <div class="value green">{comment_rate:.1f}%</div>
                    <div class="label">注释行 · {summary.get('total_comment_lines', 0):,}</div>
                    <div class="bar"><div class="bar-fill green" style="width:{comment_rate:.1f}%"></div></div>
                </div>
                <div class="metric-bar">
                    <div class="value purple">{blank_rate:.1f}%</div>
                    <div class="label">空行 · {summary.get('total_blank_lines', 0):,}</div>
                    <div class="bar"><div class="bar-fill purple" style="width:{blank_rate:.1f}%"></div></div>
                </div>
            </div>
        </div>

        <div class="grid-2">
            <div class="card">
                <div class="card-title">代码文件类型</div>
                <div class="type-list">{code_types_html}</div>
            </div>
            <div class="card">
                <div class="card-title">文档文件类型</div>
                <div class="type-list">{doc_types_html}</div>
            </div>
        </div>

        <div class="card">
            <div class="card-title">代码复杂度分析</div>
            <div class="metric-bars">
                <div class="metric-bar">
                    <div class="value blue">{complexity.get('functions', 0):,}</div>
                    <div class="label">函数/方法</div>
                </div>
                <div class="metric-bar">
                    <div class="value green">{complexity.get('avg_complexity', 0)}</div>
                    <div class="label">平均圈复杂度</div>
                </div>
                <div class="metric-bar">
                    <div class="value purple">{complexity.get('max_depth', 0)}</div>
                    <div class="label">最大嵌套深度</div>
                </div>
                <div class="metric-bar">
                    <div class="value" style="color:#f59e0b">{complexity.get('avg_func_lines', 0)}</div>
                    <div class="label">平均函数行数</div>
                </div>
            </div>
        </div>

        <div class="grid-2">
            <div class="card">
                <div class="card-title">文件大小分布</div>
                <div class="chart-wrap"><canvas id="sizeChart"></canvas></div>
            </div>
            <div class="card">
                <div class="card-title">语言分布</div>
                <div class="chart-wrap"><canvas id="langChart"></canvas></div>
            </div>
        </div>

        <div class="card">
            <div class="card-title">仓库规模</div>
            <div class="chart-wrap"><canvas id="repoChart"></canvas></div>
        </div>

        <div class="card">
            <div class="card-title">语言统计</div>
            <table>
                <thead><tr>
                    <th style="width:50px">#</th>
                    <th>语言</th>
                    <th class="text-right">仓库</th>
                    <th class="text-right">文件</th>
                    <th class="text-right">代码行</th>
                    <th class="text-right">注释率</th>
                    <th class="text-right">占比</th>
                </tr></thead>
                <tbody>{lang_table_rows}</tbody>
            </table>
        </div>

        <div class="card">
            <div class="card-title">仓库排行</div>
            <table>
                <thead><tr>
                    <th style="width:50px">#</th>
                    <th>仓库</th>
                    <th>语言</th>
                    <th class="text-right">文件</th>
                    <th class="text-right">代码行</th>
                    <th class="text-right">文档行</th>
                    <th class="text-right">注释率</th>
                    <th class="text-right">大小</th>
                </tr></thead>
                <tbody>{repo_table_rows}</tbody>
            </table>
        </div>

        {git_section}

        <div class="footer">Generated by code_statistics.py v{__version__}</div>
    </div>

    <script>
        Chart.defaults.font.family = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif";
        const colors = ['#3b82f6','#22c55e','#f59e0b','#ef4444','#8b5cf6','#ec4899','#06b6d4','#84cc16','#f97316','#6366f1'];

        new Chart(document.getElementById('sizeChart'), {{
            type: 'bar',
            data: {{
                labels: ['{size_labels["tiny"]}', '{size_labels["small"]}', '{size_labels["medium"]}', '{size_labels["large"]}', '{size_labels["huge"]}'],
                datasets: [{{
                    data: [{size_dist.get('tiny', 0)}, {size_dist.get('small', 0)}, {size_dist.get('medium', 0)}, {size_dist.get('large', 0)}, {size_dist.get('huge', 0)}],
                    backgroundColor: ['#06b6d4', '#22c55e', '#f59e0b', '#ef4444', '#8b5cf6'],
                    borderRadius: 6,
                    barThickness: 36
                }}]
            }},
            options: {{
                responsive: true,
                maintainAspectRatio: false,
                plugins: {{ legend: {{ display: false }} }},
                scales: {{
                    y: {{ grid: {{ color: '#f1f5f9' }}, ticks: {{ font: {{ size: 11 }} }} }},
                    x: {{ grid: {{ display: false }}, ticks: {{ font: {{ size: 12 }} }} }}
                }}
            }}
        }});

        new Chart(document.getElementById('langChart'), {{
            type: 'doughnut',
            data: {{ labels: {lang_labels}, datasets: [{{ data: {lang_data}, backgroundColor: colors, borderWidth: 0, hoverOffset: 4 }}] }},
            options: {{
                responsive: true,
                maintainAspectRatio: false,
                cutout: '60%',
                plugins: {{
                    legend: {{ position: 'right', labels: {{ padding: 16, usePointStyle: true, pointStyle: 'circle', font: {{ size: 12 }} }} }}
                }}
            }}
        }});

        new Chart(document.getElementById('repoChart'), {{
            type: 'bar',
            data: {{ labels: {repo_labels}, datasets: [{{ data: {repo_data}, backgroundColor: '#3b82f6', borderRadius: 6, barThickness: 24 }}] }},
            options: {{
                responsive: true,
                maintainAspectRatio: false,
                indexAxis: 'y',
                plugins: {{ legend: {{ display: false }} }},
                scales: {{
                    x: {{ grid: {{ color: '#f1f5f9' }}, ticks: {{ font: {{ size: 11 }} }} }},
                    y: {{ grid: {{ display: false }}, ticks: {{ font: {{ size: 12 }} }} }}
                }}
            }}
        }});
    </script>
</body>
</html>'''

        with open(output_file, 'w', encoding='utf-8') as f:
            f.write(html)

        if not self.args.summary:
            print(f"\n统计结果已保存到: {output_file}")

    def output_console(self, all_repos, summary):
        """输出到控制台"""
        if not self.args.summary:
            print(f"\n扫描目录: {self.current_dir}")
            print("=" * 80)
            
            for repo in sorted(all_repos, key=lambda x: x['name']):
                print(f"\n仓库: {repo['name']}")
                print(f"  主要语言: {repo['language']}")
                print(f"  文件数: {repo['files']:,}")
                print(f"  总行数: {repo['lines']:,}")
                
                # 计算并显示注释率和空行率
                total_lines = repo['lines']
                code_lines = repo.get('code_lines', 0)
                comment_lines = repo.get('comment_lines', 0)
                blank_lines = repo.get('blank_lines', 0)
                
                if total_lines > 0:
                    comment_rate = (comment_lines / total_lines) * 100
                    blank_rate = (blank_lines / total_lines) * 100
                    code_rate = (code_lines / total_lines) * 100
                    print(f"    - 纯代码行: {code_lines:,} ({code_rate:.1f}%)")
                    print(f"    - 注释行: {comment_lines:,} ({comment_rate:.1f}%)")
                    print(f"    - 空行: {blank_lines:,} ({blank_rate:.1f}%)")
                
                # 显示前5个文件类型
                sorted_exts = sorted(repo['details'].items(),
                                   key=lambda x: x[1]['lines'], reverse=True)[:5]
                if sorted_exts:
                    print("  主要文件类型:")
                    for ext, data in sorted_exts:
                        print(f"    {ext}: {data['files']} 个文件, {data['lines']:,} 行")

                # 显示 Git 信息
                if 'git' in repo and repo['git']:
                    git = repo['git']
                    print("  Git 信息:")
                    if git.get('branch'):
                        print(f"    分支: {git['branch']}")
                    if git.get('last_commit_date'):
                        print(f"    最后提交: {git['last_commit_date']}")
                    if git.get('last_commit_author'):
                        print(f"    提交者: {git['last_commit_author']}")
                    if git.get('total_commits'):
                        print(f"    提交总数: {git['total_commits']:,}")
                    if git.get('contributors'):
                        print(f"    贡献者数: {git['contributors']}")
        
        # 输出总体统计
        print("\n" + "=" * 80)
        print("总体统计")
        print("=" * 80)
        print(f"仓库总数: {summary['total_repos']}")
        print(f"代码文件数: {summary['total_files']:,}")
        print(f"代码总行数: {summary['total_lines']:,}")

        # 显示代码类型分布
        code_details = summary.get('code_details', {})
        if code_details:
            sorted_codes = sorted(code_details.items(), key=lambda x: x[1]['lines'], reverse=True)
            print("  代码类型分布:")
            for ext, details in sorted_codes:
                print(f"    {ext}: {details['files']} 个文件, {details['lines']:,} 行")

        # 显示代码组成分析
        total_code_lines = summary.get('total_code_lines', 0)
        total_comment_lines = summary.get('total_comment_lines', 0)
        total_blank_lines = summary.get('total_blank_lines', 0)
        if summary['total_lines'] > 0:
            code_rate = (total_code_lines / summary['total_lines']) * 100
            comment_rate = (total_comment_lines / summary['total_lines']) * 100
            blank_rate = (total_blank_lines / summary['total_lines']) * 100
            print("代码组成:")
            print(f"  - 纯代码行: {total_code_lines:,} ({code_rate:.1f}%)")
            print(f"  - 注释行: {total_comment_lines:,} ({comment_rate:.1f}%)")
            print(f"  - 空行: {total_blank_lines:,} ({blank_rate:.1f}%)")
        
        print(f"文档文件数: {summary.get('total_doc_files', 0):,}")
        print(f"文档总行数: {summary.get('total_doc_lines', 0):,}")

        # 显示文档类型详情
        doc_details = summary.get('doc_details', {})
        if doc_details:
            sorted_docs = sorted(doc_details.items(), key=lambda x: x[1]['lines'], reverse=True)
            print("  文档类型分布:")
            for ext, details in sorted_docs:
                print(f"    {ext}: {details['files']} 个文件, {details['lines']:,} 行")
        print("\n大小统计:")
        print(f"  代码大小: {self.format_size(summary.get('total_size', 0))}")
        print(f"  文档大小: {self.format_size(summary.get('total_doc_size', 0))}")
        print(f"  仓库总大小: {self.format_size(summary.get('total_all_size', summary.get('total_size', 0)))}")
        
        # 按语言统计
        print("\n按语言统计:")
        sorted_languages = sorted(summary['by_language'].items(), 
                                key=lambda x: x[1]['lines'], reverse=True)
        for lang, data in sorted_languages:
            percentage = (data['lines'] / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
            print(f"  {lang}:")
            print(f"    仓库数: {data['repos']}")
            print(f"    文件数: {data['files']:,}")
            print(f"    代码行数: {data['lines']:,} ({percentage:.1f}%)")
        
        # 按仓库排序
        print("\n仓库排行（按代码行数）:")
        sorted_repos = sorted(all_repos, key=lambda x: x['lines'], reverse=True)
        
        # 应用top限制
        if self.args.top:
            sorted_repos = sorted_repos[:self.args.top]
        else:
            sorted_repos = sorted_repos[:10]
        
        for i, repo in enumerate(sorted_repos, 1):
            print(f"  {i}. {repo['name']} ({repo['language']}): {repo['lines']:,} 行")
    
    def run(self):
        """运行统计"""
        start_time = time.time()
        
        # 分析所有仓库
        all_repos = self.analyze_all_repos()
        
        # 生成摘要
        summary = self.generate_summary(all_repos)
        
        # 根据输出格式输出结果
        if self.args.output == 'json':
            self.output_json(all_repos, summary)
        elif self.args.output == 'csv':
            self.output_csv(all_repos, summary)
        elif self.args.output == 'markdown':
            self.output_markdown(all_repos, summary)
        elif self.args.output == 'html':
            self.output_html(all_repos, summary)
        
        # 始终输出到控制台（除非指定了其他格式且设置了quiet）
        if not self.args.quiet:
            self.output_console(all_repos, summary)
        
        elapsed_time = time.time() - start_time
        if self.args.verbose:
            print(f"\n统计完成，耗时: {elapsed_time:.2f} 秒")

def load_config_file(config_path=None):
    """加载配置文件

    配置文件搜索顺序:
    1. 命令行指定的路径
    2. 当前目录的 .code_stats.yaml 或 .code_stats.yml
    3. 当前目录的 .code_stats.json
    4. 用户目录的 ~/.code_stats.yaml

    返回配置字典，如果没有找到配置文件则返回空字典
    """
    config = {}

    # 确定配置文件路径
    search_paths = []
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
                config = yaml.safe_load(f) or {}
            else:
                # 如果没有 yaml 模块，尝试用 JSON 解析
                print(f"警告：未安装 PyYAML，无法解析 {config_file}")
                print("请运行: pip install pyyaml")
                return config

        print(f"已加载配置文件: {config_file}")
    except Exception as e:
        print(f"警告：读取配置文件失败 {config_file}: {e}")

    return config


def merge_args_with_config(args, config):
    """将配置文件中的设置合并到参数中（命令行参数优先）"""
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
            if current_value is None or current_value == False or current_value == []:
                setattr(args, arg_attr, config[config_key])

    # 处理特殊的 dirs 参数（列表类型）
    if 'dirs' in config and not args.dirs:
        args.dirs = config['dirs']

    # 处理特殊的 repo 参数
    if 'repo' in config and not args.repo:
        args.repo = config['repo']

    return args


def show_supported_languages():
    """显示支持的编程语言列表"""
    # 收集所有唯一的语言
    languages = sorted(set(LANGUAGE_MAP.values()))

    print("支持的编程语言列表:")
    print("=" * 60)

    # 按语言分组显示扩展名
    lang_to_exts = {}
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


def main():
    """主函数"""
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
    
    # 添加版本号参数
    parser.add_argument('--version', '-V', action='version',
                        version=f'%(prog)s {__version__}')

    # 显示支持的语言
    parser.add_argument('--help-lang', action='store_true',
                        help='显示支持的编程语言列表')
    
    # 基本参数
    parser.add_argument('--all', '-a', action='store_true',
                        help='统计所有文件（包括依赖和文档）')
    parser.add_argument('--excludes', type=str,
                        help='额外的排除模式，用逗号分隔（支持通配符）')
    parser.add_argument('--exclude-files', type=str,
                        help='排除文件的正则表达式模式，用逗号分隔（如: .*_test\\.py$,.*\\.bak$）')
    parser.add_argument('--exclude-dirs', type=str,
                        help='排除目录的正则表达式模式，用逗号分隔（如: .*/test/.*,.*/backup/.*）')
    parser.add_argument('--lang', type=str,
                        help='指定统计的编程语言，用逗号分隔（如: python,go,javascript）')
    parser.add_argument('--dirs', '-d', type=str, action='append',
                        help='指定要统计的目录（可多次使用，如: -d api -d admin）')
    
    # 输出格式
    parser.add_argument('--output', '-o', choices=['json', 'csv', 'markdown', 'html'],
                        help='输出格式（默认输出到控制台）')
    parser.add_argument('--output-file', '-O', type=str,
                        help='输出文件名（默认根据格式自动命名）')
    
    # 过滤选项
    parser.add_argument('--repo', type=str,
                        help='指定特定仓库，用逗号分隔')
    parser.add_argument('--min-lines', type=int,
                        help='只统计行数大于指定值的文件')
    parser.add_argument('--max-lines', type=int,
                        help='只统计行数小于指定值的文件')
    
    # 显示选项
    parser.add_argument('--verbose', '-v', action='store_true',
                        help='显示详细信息')
    parser.add_argument('--summary', '-s', action='store_true',
                        help='只显示摘要信息')
    parser.add_argument('--quiet', '-q', action='store_true',
                        help='安静模式（不输出到控制台）')
    parser.add_argument('--sort', choices=['lines', 'files', 'name'],
                        default='lines', help='排序方式（默认按行数）')
    parser.add_argument('--top', type=int,
                        help='只显示前N个结果')
    
    # 性能选项
    parser.add_argument('--parallel', '-p', action='store_true',
                        help='使用多线程并行处理')

    # 配置文件选项
    parser.add_argument('--config', '-c', type=str,
                        help='指定配置文件路径（支持 .yaml/.yml/.json）')
    parser.add_argument('--no-config', action='store_true',
                        help='不加载配置文件')

    # 高级选项
    parser.add_argument('--include-files', type=str,
                        help='包含文件的正则表达式模式，用逗号分隔')
    parser.add_argument('--depth', type=int,
                        help='目录扫描深度限制（0 表示无限制）')
    parser.add_argument('--git-info', action='store_true',
                        help='显示 Git 仓库信息（最后提交时间、作者等）')
    parser.add_argument('--no-smart-exclude', action='store_true',
                        help='禁用智能排除，使用完整的排除目录列表（默认启用智能排除）')
    parser.add_argument('--no-gitignore', action='store_true',
                        help='禁用 .gitignore 规则过滤（默认启用）')

    args = parser.parse_args()

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