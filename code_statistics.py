#!/usr/bin/env python3
"""
代码统计脚本 - 统计当前目录下所有仓库的代码行数
支持多种参数自定义统计行为
"""

__version__ = "1.3.1"

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

# 需要排除的目录模式
DEFAULT_EXCLUDE_DIRS = {
    # Python
    '__pycache__', '.venv', 'venv', 'env', '.env', 'virtualenv',
    '.pytest_cache', '.mypy_cache', '.tox', 'dist', 'build',
    '*.egg-info', '.eggs', 'site-packages', 'htmlcov',
    '.hypothesis', '.coverage', '.ruff_cache',
    
    # JavaScript/Node
    'node_modules', '.npm', 'bower_components', 'jspm_packages',
    '.yarn', '.pnp', 'coverage', '.nyc_output', '.next',
    '.nuxt', '.cache', 'dist', 'out', '.parcel-cache',
    '.turbo', '.vercel', '.netlify', '.sveltekit',
    
    # Go
    'vendor', 'pkg', '.go', 'bin',
    
    # Java/JVM
    'target', '.gradle', '.mvn', 'out', 'classes',
    '.m2', 'generated', 'generated-sources', 'generated-test-sources',
    
    # .NET
    'bin', 'obj', '.vs', 'packages', '.nuget',
    '_ReSharper*', '*.resharper*',
    
    # Ruby
    '.bundle', 'gems', '.sass-cache', '_site',
    
    # PHP
    'vendor', '.phpunit.result.cache', '.php_cs.cache',
    '.phpstan', '.psalm',
    
    # Rust
    'target', 'Cargo.lock',
    
    # Dart/Flutter
    '.dart_tool', '.flutter-plugins', '.flutter-plugins-dependencies',
    'build', '.pub-cache', '.pub',
    
    # iOS/macOS
    'Pods', '.build', 'xcuserdata', '*.xcworkspace',
    'DerivedData', '.swiftpm',
    
    # Android
    '.gradle', 'gradle', 'build', '.android',
    'local.properties', '*.iml',
    
    # Elixir
    '_build', 'deps', '.fetch', 'erl_crash.dump',
    '*.ez', 'priv/static',
    
    # General
    '.git', '.svn', '.hg', '.bzr', '_darcs',
    '.idea', '.vscode', '.vs', '.settings', '.project',
    'logs', 'log', 'tmp', 'temp', 'cache', '.cache',
    '.DS_Store', 'Thumbs.db', 'desktop.ini',
    '*.pyc', '*.pyo', '*.swp', '*.swo', '*~',
    '.terraform', '.vagrant', '.docker'
}

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

class CodeStatistics:
    def __init__(self, args):
        self.args = args
        self.exclude_dirs = set(DEFAULT_EXCLUDE_DIRS)
        self.current_dir = os.getcwd()
        self.progress_count = 0
        self.total_repos = 0
        
        # 处理额外的排除模式
        if args.excludes:
            for pattern in args.excludes.split(','):
                self.exclude_dirs.add(pattern.strip())
        
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
        
        return False
    
    def is_code_file(self, file_path):
        """检查文件是否是代码文件"""
        file_name = os.path.basename(file_path)
        file_ext = Path(file_path).suffix.lower()

        # 排除二进制文件
        if file_ext in BINARY_EXTENSIONS:
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
        """分析文件内容，返回详细的行数统计"""
        # 先检测是否为二进制文件
        if self.is_binary_file(file_path):
            return {'total': 0, 'code': 0, 'comment': 0, 'blank': 0, 'size': 0}
            
        try:
            file_size = os.path.getsize(file_path)
            with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
                lines = f.readlines()
                
            total_lines = len(lines)
            code_lines = 0
            comment_lines = 0
            blank_lines = 0
            
            # 获取文件扩展名以确定注释风格
            ext = Path(file_path).suffix.lower()
            lang = LANGUAGE_MAP.get(ext, 'other')
            
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
            
            # 应用行数过滤
            if self.args.min_lines and total_lines < self.args.min_lines:
                return {'total': 0, 'code': 0, 'comment': 0, 'blank': 0, 'size': 0}
            
            if self.args.max_lines and total_lines > self.args.max_lines:
                return {'total': 0, 'code': 0, 'comment': 0, 'blank': 0, 'size': 0}
            
            return {
                'total': total_lines,
                'code': code_lines,
                'comment': comment_lines,
                'blank': blank_lines,
                'size': file_size
            }
            
        except Exception as e:
            if self.args.verbose:
                print(f"Error reading {file_path}: {e}")
            return {'total': 0, 'code': 0, 'comment': 0, 'blank': 0, 'size': 0}
    
    def count_lines(self, file_path):
        """统计文件行数（向后兼容）"""
        result = self.analyze_file_content(file_path)
        return result['total']
    
    def analyze_repository(self, repo_path):
        """分析单个仓库的代码统计"""
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
            'file_details': file_details if self.args.verbose else []
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
                                'file_details': stats.get('file_details', [])
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
                        'file_details': stats.get('file_details', [])
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
            'by_language': dict(language_summary)
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
            f.write(f"# 代码统计报告\n\n")
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
        """输出HTML格式"""
        output_file = self.args.output_file or 'code_statistics.html'
        
        with open(output_file, 'w', encoding='utf-8') as f:
            # HTML头部
            f.write("""<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>代码统计报告</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <script src="https://cdn.jsdelivr.net/npm/chart.js"></script>
    <script src="https://cdn.jsdelivr.net/npm/chartjs-plugin-datalabels@2"></script>
    <style>
        /* 自定义样式 */
        :root {
            --primary-gradient: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
        }
        
        /* 暗色模式支持 */
        @media (prefers-color-scheme: dark) {
            :root {
                --primary-gradient: linear-gradient(135deg, #4c1d95 0%, #5b21b6 100%);
            }
        }
        
        /* 图表动画 */
        @keyframes fadeIn {
            from { opacity: 0; transform: translateY(20px); }
            to { opacity: 1; transform: translateY(0); }
        }
        
        .animate-fade-in {
            animation: fadeIn 0.6s ease-out;
        }
        
        /* 进度条动画 */
        @keyframes progressAnimation {
            from { width: 0; }
        }
        
        .progress-animation {
            animation: progressAnimation 1s ease-out;
        }
        
        /* 数字动画 */
        @property --num {
            syntax: '<integer>';
            initial-value: 0;
            inherits: false;
        }
        
        .counter-animation {
            counter-reset: num var(--num);
            animation: counter 2s ease-out;
        }
        
        .counter-animation::after {
            content: counter(num);
        }
        
        @keyframes counter {
            from { --num: 0; }
        }
        
        /* 打印样式 */
        @media print {
            .no-print { display: none !important; }
            body { background: white !important; }
            .shadow-lg { box-shadow: none !important; }
        }
    </style>
</head>
<body class="bg-gray-50 dark:bg-gray-900 text-gray-900 dark:text-gray-100">
    <div class="container mx-auto px-4 py-8 max-w-7xl">
""")
            
            # 头部信息
            f.write(f"""
        <div class="bg-gradient-to-r from-indigo-600 to-purple-600 rounded-xl shadow-xl p-8 mb-8 text-white animate-fade-in">
            <h1 class="text-4xl font-bold mb-4 flex items-center justify-center">
                <span class="mr-3">📊</span> 代码统计报告
            </h1>
            <div class="flex flex-wrap justify-center gap-4 text-sm opacity-90">
                <div class="flex items-center">
                    <svg class="w-4 h-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z"></path>
                    </svg>
                    {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}
                </div>
                <div class="flex items-center">
                    <svg class="w-4 h-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"></path>
                    </svg>
                    {self.current_dir}
                </div>
            </div>
        </div>
""")
            
            # 统计卡片
            avg_lines_per_repo = summary['total_lines'] // summary['total_repos'] if summary['total_repos'] > 0 else 0
            avg_lines_per_file = summary['total_lines'] // summary['total_files'] if summary['total_files'] > 0 else 0
            
            # 计算新的统计数据
            comment_rate = (summary.get('total_comment_lines', 0) / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
            blank_rate = (summary.get('total_blank_lines', 0) / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
            code_rate = (summary.get('total_code_lines', summary['total_lines']) / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
            
            f.write("""
        <!-- 统计卡片 -->
        <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6 mb-8">
            <!-- 仓库总数 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 hover:shadow-xl transition-shadow animate-fade-in" style="animation-delay: 0.1s">
                <div class="flex items-center justify-between mb-4">
                    <div class="text-5xl">🗂️</div>
                    <div class="text-right">
                        <div class="text-3xl font-bold text-gray-800 dark:text-gray-200">{:,}</div>
                        <div class="text-sm text-gray-600 dark:text-gray-400">仓库总数</div>
                    </div>
                </div>
            </div>
            
            <!-- 文件总数 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 hover:shadow-xl transition-shadow animate-fade-in" style="animation-delay: 0.2s">
                <div class="flex items-center justify-between mb-4">
                    <div class="text-5xl">📄</div>
                    <div class="text-right">
                        <div class="text-3xl font-bold text-gray-800 dark:text-gray-200">{:,}</div>
                        <div class="text-sm text-gray-600 dark:text-gray-400">文件总数</div>
                    </div>
                </div>
            </div>
            
            <!-- 代码总行数 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 hover:shadow-xl transition-shadow animate-fade-in" style="animation-delay: 0.3s">
                <div class="flex items-center justify-between mb-4">
                    <div class="text-5xl">📝</div>
                    <div class="text-right">
                        <div class="text-3xl font-bold text-gray-800 dark:text-gray-200">{:,}</div>
                        <div class="text-sm text-gray-600 dark:text-gray-400">代码总行数</div>
                    </div>
                </div>
            </div>
            
            <!-- 总大小 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 hover:shadow-xl transition-shadow animate-fade-in" style="animation-delay: 0.4s">
                <div class="flex items-center justify-between mb-4">
                    <div class="text-5xl">💾</div>
                    <div class="text-right">
                        <div class="text-3xl font-bold text-gray-800 dark:text-gray-200">{}</div>
                        <div class="text-sm text-gray-600 dark:text-gray-400">总大小</div>
                    </div>
                </div>
            </div>
        </div>
        
        <!-- 代码质量指标 -->
        <div class="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
            <!-- 注释率 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 animate-fade-in" style="animation-delay: 0.5s">
                <div class="flex items-center justify-between mb-4">
                    <div class="text-lg font-semibold text-gray-700 dark:text-gray-300">💬 注释率</div>
                    <div class="text-2xl font-bold text-green-600">{:.1f}%</div>
                </div>
                <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2.5">
                    <div class="bg-green-600 h-2.5 rounded-full progress-animation" style="width: {:.1f}%"></div>
                </div>
                <div class="text-xs text-gray-600 dark:text-gray-400 mt-2">{:,} 注释行</div>
            </div>
            
            <!-- 代码率 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 animate-fade-in" style="animation-delay: 0.6s">
                <div class="flex items-center justify-between mb-4">
                    <div class="text-lg font-semibold text-gray-700 dark:text-gray-300">⌨️ 代码率</div>
                    <div class="text-2xl font-bold text-blue-600">{:.1f}%</div>
                </div>
                <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2.5">
                    <div class="bg-blue-600 h-2.5 rounded-full progress-animation" style="width: {:.1f}%"></div>
                </div>
                <div class="text-xs text-gray-600 dark:text-gray-400 mt-2">{:,} 实际代码行</div>
            </div>
            
            <!-- 空行率 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 animate-fade-in" style="animation-delay: 0.7s">
                <div class="flex items-center justify-between mb-4">
                    <div class="text-lg font-semibold text-gray-700 dark:text-gray-300">📏 空行率</div>
                    <div class="text-2xl font-bold text-purple-600">{:.1f}%</div>
                </div>
                <div class="w-full bg-gray-200 dark:bg-gray-700 rounded-full h-2.5">
                    <div class="bg-purple-600 h-2.5 rounded-full progress-animation" style="width: {:.1f}%"></div>
                </div>
                <div class="text-xs text-gray-600 dark:text-gray-400 mt-2">{:,} 空行</div>
            </div>
        </div>
""".format(
                summary['total_repos'], 
                summary['total_files'], 
                summary['total_lines'],
                self.format_size(summary.get('total_size', 0)),
                comment_rate, comment_rate, summary.get('total_comment_lines', 0),
                code_rate, code_rate, summary.get('total_code_lines', summary['total_lines']),
                blank_rate, blank_rate, summary.get('total_blank_lines', 0)
            ))
            
            # 图表部分
            sorted_languages = sorted(summary['by_language'].items(), 
                                    key=lambda x: x[1]['lines'], reverse=True)[:10]
            
            f.write("""
        <!-- 图表部分 -->
        <div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mb-8">
            <!-- 语言分布饼图 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 animate-fade-in" style="animation-delay: 0.8s">
                <h3 class="text-xl font-semibold text-gray-800 dark:text-gray-200 mb-4">🌐 语言分布</h3>
                <div style="position: relative; height: 300px;">
                    <canvas id="languageChart"></canvas>
                </div>
            </div>
            
            <!-- 仓库大小柱状图 -->
            <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 animate-fade-in" style="animation-delay: 0.9s">
                <h3 class="text-xl font-semibold text-gray-800 dark:text-gray-200 mb-4">📊 仓库大小对比 (TOP 10)</h3>
                <div style="position: relative; height: 300px;">
                    <canvas id="repoChart"></canvas>
                </div>
            </div>
        </div>
        
        <!-- 代码组成分析 -->
        <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 mb-8 animate-fade-in" style="animation-delay: 1.0s">
            <h3 class="text-xl font-semibold text-gray-800 dark:text-gray-200 mb-4">📋 代码组成分析</h3>
            <div style="position: relative; height: 400px;">
                <canvas id="compositionChart"></canvas>
            </div>
        </div>
""")
            
            # 语言统计表
            f.write("""
        <!-- 语言统计表 -->
        <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 mb-8 animate-fade-in" style="animation-delay: 1.1s">
            <h3 class="text-xl font-semibold text-gray-800 dark:text-gray-200 mb-4">📊 语言详细统计</h3>
            <div class="overflow-x-auto">
                <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                    <thead class="bg-gray-50 dark:bg-gray-900">
                        <tr>
                            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">排名</th>
                            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">语言</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">仓库数</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">文件数</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">代码行数</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">注释率</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">占比</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">大小</th>
                        </tr>
                    </thead>
                    <tbody class="bg-white dark:bg-gray-800 divide-y divide-gray-200 dark:divide-gray-700">
""")
            
            for idx, (lang, data) in enumerate(sorted_languages, 1):
                percentage = (data['lines'] / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
                comment_rate = (data.get('comment_lines', 0) / data['lines'] * 100) if data['lines'] > 0 else 0
                lang_emoji = self.get_language_emoji(lang)
                
                f.write(f"""
                        <tr class="hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors">
                            <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900 dark:text-gray-100">{idx}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100">
                                <span class="text-lg mr-2">{lang_emoji}</span>{lang}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">{data['repos']}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">{data['files']:,}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">{data['lines']:,}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">
                                <span class="text-green-600 dark:text-green-400">{comment_rate:.1f}%</span>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">
                                <div class="flex items-center justify-end">
                                    <span class="mr-2">{percentage:.1f}%</span>
                                    <div class="w-16 bg-gray-200 dark:bg-gray-700 rounded-full h-2.5">
                                        <div class="bg-blue-600 h-2.5 rounded-full" style="width: {percentage:.1f}%"></div>
                                    </div>
                                </div>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">
                                {self.format_size(data.get('size', 0))}
                            </td>
                        </tr>
""")
            
            f.write("""
                    </tbody>
                </table>
            </div>
        </div>
""")
            
            # 仓库排行榜
            sorted_by_lines = sorted(all_repos, key=lambda x: x['lines'], reverse=True)[:10]
            
            f.write("""
        <!-- 仓库排行榜 -->
        <div class="bg-white dark:bg-gray-800 rounded-xl shadow-lg p-6 mb-8 animate-fade-in" style="animation-delay: 1.2s">
            <h3 class="text-xl font-semibold text-gray-800 dark:text-gray-200 mb-4">🏆 仓库排行榜 (TOP 10)</h3>
            <div class="overflow-x-auto">
                <table class="min-w-full divide-y divide-gray-200 dark:divide-gray-700">
                    <thead class="bg-gray-50 dark:bg-gray-900">
                        <tr>
                            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">排名</th>
                            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">仓库名</th>
                            <th class="px-6 py-3 text-left text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">主要语言</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">文件数</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">代码行数</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">文档行数</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">注释率</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">占比</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">代码大小</th>
                            <th class="px-6 py-3 text-right text-xs font-medium text-gray-500 dark:text-gray-400 uppercase tracking-wider">总大小</th>
                        </tr>
                    </thead>
                    <tbody class="bg-white dark:bg-gray-800 divide-y divide-gray-200 dark:divide-gray-700">
""")
            
            for i, repo in enumerate(sorted_by_lines, 1):
                percentage = (repo['lines'] / summary['total_lines'] * 100) if summary['total_lines'] > 0 else 0
                comment_rate = (repo.get('comment_lines', 0) / repo['lines'] * 100) if repo['lines'] > 0 else 0
                medal = "🥇" if i == 1 else "🥈" if i == 2 else "🥉" if i == 3 else f"{i}"
                lang_emoji = self.get_language_emoji(repo['language'])
                
                f.write(f"""
                        <tr class="hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors">
                            <td class="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900 dark:text-gray-100">
                                <span class="text-2xl">{medal}</span>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm font-semibold text-gray-900 dark:text-gray-100">
                                {repo['name']}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100">
                                <span class="text-lg mr-2">{lang_emoji}</span>{repo['language']}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">{repo['files']:,}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">{repo['lines']:,}</td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">
                                <span class="text-purple-600 dark:text-purple-400">{repo.get('doc_lines', 0):,}</span>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">
                                <span class="text-green-600 dark:text-green-400">{comment_rate:.1f}%</span>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">
                                <div class="flex items-center justify-end">
                                    <span class="mr-2">{percentage:.1f}%</span>
                                    <div class="w-16 bg-gray-200 dark:bg-gray-700 rounded-full h-2.5">
                                        <div class="bg-indigo-600 h-2.5 rounded-full" style="width: {percentage:.1f}%"></div>
                                    </div>
                                </div>
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">
                                {self.format_size(repo.get('size', 0))}
                            </td>
                            <td class="px-6 py-4 whitespace-nowrap text-sm text-gray-900 dark:text-gray-100 text-right">
                                {self.format_size(repo.get('total_size', repo.get('size', 0)))}
                            </td>
                        </tr>
""")
            
            f.write("""
                </tbody>
            </table>
        </div>
""")
            
            # 仓库详细信息部分（现在不需要，信息已足够）
            # 如果需要可以添加更详细的仓库卡片
            
            # 页脚
            f.write("""
        <div class="footer">
            <p>Generated by code_statistics.py</p>
        </div>
    </div>
    
    <script>
        // 配置 Chart.js 默认字体
        Chart.defaults.font.family = '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif';
        
        // 语言分布饼图
        const languageCtx = document.getElementById('languageChart').getContext('2d');
        const languageData = {
            labels: [""" + ", ".join([f"'{lang}'" for lang, _ in sorted_languages]) + """],
            datasets: [{
                data: [""" + ", ".join([str(data['lines']) for _, data in sorted_languages]) + """],
                backgroundColor: [
                    '#3B82F6', '#10B981', '#F59E0B', '#EF4444', '#8B5CF6',
                    '#EC4899', '#14B8A6', '#F97316', '#6366F1', '#84CC16'
                ],
                borderWidth: 2,
                borderColor: '#fff'
            }]
        };
        
        new Chart(languageCtx, {
            type: 'doughnut',
            data: languageData,
            options: {
                responsive: true,
                maintainAspectRatio: false,
                plugins: {
                    legend: {
                        position: 'right',
                        labels: {
                            padding: 15,
                            font: { size: 13 },
                            generateLabels: function(chart) {
                                const data = chart.data;
                                const total = data.datasets[0].data.reduce((a, b) => a + b, 0);
                                return data.labels.map((label, i) => ({
                                    text: `${label} (${((data.datasets[0].data[i] / total) * 100).toFixed(1)}%)`,
                                    fillStyle: data.datasets[0].backgroundColor[i],
                                    hidden: false,
                                    index: i
                                }));
                            }
                        }
                    },
                    tooltip: {
                        callbacks: {
                            label: function(context) {
                                const total = context.dataset.data.reduce((a, b) => a + b, 0);
                                const percentage = ((context.parsed / total) * 100).toFixed(1);
                                return context.label + ': ' + context.parsed.toLocaleString() + ' 行 (' + percentage + '%)';
                            }
                        }
                    }
                }
            }
        });
        
        // 仓库大小对比柱状图
        const repoCtx = document.getElementById('repoChart').getContext('2d');
        const repoLabels = [""" + ", ".join([f"'{repo['name']}'" for repo in sorted_by_lines]) + """];
        const repoData = [""" + ", ".join([str(repo['lines']) for repo in sorted_by_lines]) + """];
        
        new Chart(repoCtx, {
            type: 'bar',
            data: {
                labels: repoLabels.slice(0, 10),  // Top 10
                datasets: [{
                    label: '代码行数',
                    data: repoData.slice(0, 10),
                    backgroundColor: '#3B82F6',
                    borderColor: '#2563EB',
                    borderWidth: 1
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                scales: {
                    y: {
                        beginAtZero: true,
                        ticks: {
                            callback: function(value) {
                                return value.toLocaleString();
                            }
                        }
                    },
                    x: {
                        ticks: {
                            maxRotation: 45,
                            minRotation: 45
                        }
                    }
                },
                plugins: {
                    legend: {
                        display: false
                    },
                    tooltip: {
                        callbacks: {
                            label: function(context) {
                                return '代码行数: ' + context.parsed.y.toLocaleString();
                            }
                        }
                    }
                }
            }
        });
        
        // 代码组成分析（堆叠柱状图）
        const compositionCtx = document.getElementById('compositionChart').getContext('2d');
        const compositionData = {
            labels: [""" + ", ".join([f"'{repo['name']}'" for repo in sorted_by_lines[:10]]) + """],
            code: [""" + ", ".join([str(repo.get('code_lines', repo['lines'])) for repo in sorted_by_lines[:10]]) + """],
            comment: [""" + ", ".join([str(repo.get('comment_lines', 0)) for repo in sorted_by_lines[:10]]) + """],
            blank: [""" + ", ".join([str(repo.get('blank_lines', 0)) for repo in sorted_by_lines[:10]]) + """],
            doc: [""" + ", ".join([str(repo.get('doc_lines', 0)) for repo in sorted_by_lines[:10]]) + """]
        };
        
        new Chart(compositionCtx, {
            type: 'bar',
            data: {
                labels: compositionData.labels,
                datasets: [
                    {
                        label: '代码行',
                        data: compositionData.code,
                        backgroundColor: '#3B82F6',
                        borderColor: '#2563EB',
                        borderWidth: 1
                    },
                    {
                        label: '注释行',
                        data: compositionData.comment,
                        backgroundColor: '#10B981',
                        borderColor: '#059669',
                        borderWidth: 1
                    },
                    {
                        label: '空行',
                        data: compositionData.blank,
                        backgroundColor: '#F59E0B',
                        borderColor: '#D97706',
                        borderWidth: 1
                    },
                    {
                        label: '文档行',
                        data: compositionData.doc,
                        backgroundColor: '#8B5CF6',
                        borderColor: '#7C3AED',
                        borderWidth: 1
                    }
                ]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                scales: {
                    x: {
                        stacked: true,
                        ticks: {
                            maxRotation: 45,
                            minRotation: 45
                        }
                    },
                    y: {
                        stacked: true,
                        beginAtZero: true,
                        ticks: {
                            callback: function(value) {
                                return value.toLocaleString();
                            }
                        }
                    }
                },
                plugins: {
                    tooltip: {
                        callbacks: {
                            label: function(context) {
                                return context.dataset.label + ': ' + context.parsed.y.toLocaleString() + ' 行';
                            }
                        }
                    }
                }
            }
        });
        
        // 暗色模式切换
        const darkModeToggle = document.getElementById('darkModeToggle');
        const htmlElement = document.documentElement;
        
        // 检查本地存储的主题设置
        const currentTheme = localStorage.getItem('theme') || 'light';
        if (currentTheme === 'dark') {
            htmlElement.classList.add('dark');
        }
        
        darkModeToggle.addEventListener('click', () => {
            htmlElement.classList.toggle('dark');
            const theme = htmlElement.classList.contains('dark') ? 'dark' : 'light';
            localStorage.setItem('theme', theme);
            
            // 更新图表颜色
            const isDark = theme === 'dark';
            Chart.defaults.color = isDark ? '#E5E7EB' : '#374151';
            Chart.defaults.borderColor = isDark ? 'rgba(255, 255, 255, 0.1)' : 'rgba(0, 0, 0, 0.1)';
            Chart.defaults.plugins.legend.labels.color = isDark ? '#E5E7EB' : '#374151';
            
            // 重新渲染图表
            window.location.reload();
        });
    </script>
</body>
</html>
""")
        
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
        
        # 显示代码组成分析
        total_code_lines = summary.get('total_code_lines', 0)
        total_comment_lines = summary.get('total_comment_lines', 0)
        total_blank_lines = summary.get('total_blank_lines', 0)
        if summary['total_lines'] > 0:
            code_rate = (total_code_lines / summary['total_lines']) * 100
            comment_rate = (total_comment_lines / summary['total_lines']) * 100
            blank_rate = (total_blank_lines / summary['total_lines']) * 100
            print(f"  - 纯代码行: {total_code_lines:,} ({code_rate:.1f}%)")
            print(f"  - 注释行: {total_comment_lines:,} ({comment_rate:.1f}%)")
            print(f"  - 空行: {total_blank_lines:,} ({blank_rate:.1f}%)")
        
        print(f"文档文件数: {summary.get('total_doc_files', 0):,}")
        print(f"文档总行数: {summary.get('total_doc_lines', 0):,}")
        print(f"\n大小统计:")
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

配置文件示例 (.code_stats.yaml):
  excludes: "*test*,*mock*"
  lang: python,go,javascript
  output: html
  parallel: true
  depth: 5
  git_info: true
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