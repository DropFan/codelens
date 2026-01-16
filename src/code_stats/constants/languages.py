"""语言映射和项目标记"""

from typing import Dict, List

# 项目类型检测标记文件
PROJECT_MARKERS: Dict[str, List[str]] = {
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

# 语言映射
LANGUAGE_MAP: Dict[str, str] = {
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
    '.mat': 'matlab', '.mlx': 'matlab',

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
