"""文件扩展名常量"""

from typing import Set

# 文档文件扩展名
DOC_EXTENSIONS: Set[str] = {
    '.md', '.markdown', '.rst', '.txt', '.doc', '.docx',
    '.pdf', '.odt', '.rtf', '.tex', '.wiki', '.org',
    '.adoc', '.asciidoc', '.pod', '.man', '.textile',
    'readme', 'license', 'changelog', 'authors', 'contributors',
    'notice', 'history', 'changes', 'install', 'todo'
}

# 配置文件扩展名（需要排除）
CONFIG_EXTENSIONS: Set[str] = {
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
BINARY_EXTENSIONS: Set[str] = {
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
    '.m4v', '.mpg', '.mpeg', '.3gp', '.3g2', '.mxf',  # .ts removed (conflicts with TypeScript)
    '.m2ts', '.vob', '.ogv', '.drc', '.mng', '.qt', '.yuv',
    '.rm', '.rmvb', '.asf', '.amv', '.m4p', '.svi',

    # 字体文件
    '.ttf', '.otf', '.woff', '.woff2', '.eot', '.sfnt', '.fon',
    '.fnt', '.font', '.ttc', '.pfb', '.pfm', '.afm',

    # 数据库文件
    '.db', '.sqlite', '.sqlite3', '.mdb', '.accdb', '.dbf',

    # 办公文档（通常是二进制）
    '.doc', '.docx', '.xls', '.xlsx', '.ppt', '.pptx',
    '.odt', '.ods', '.odp', '.pdf',

    # 模型和数据文件（序列化格式）
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
CODE_EXTENSIONS: Set[str] = {
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
    '.vue', '.svelte', '.astro',

    # Mobile
    '.dart', '.gradle', '.pro',

    # Database
    '.sql', '.plsql', '.tsql', '.psql', '.mysql',

    # Data Science / ML
    '.r', '.R', '.rmd', '.Rmd', '.jl', '.ipynb',
    '.mlx',

    # Functional Programming
    '.hs', '.lhs', '.elm', '.ml', '.mli',
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
    '.P', '.ecl', '.ncl', '.sml', '.sig',

    # Markup/Template
    '.jsp', '.asp', '.aspx', '.ejs', '.pug', '.jade', '.hbs',
    '.mustache', '.twig', '.liquid', '.jinja', '.j2',

    # Build/Make
    '.cmake', '.mk', '.mak', '.gnumakefile', '.makefile',
    '.ninja', '.gn', '.gni', '.bazel', '.bzl', '.BUILD',

    # Documentation (that contains code)
    '.tex', '.cls', '.sty', '.bib',

    # Game Development
    '.shader', '.cginc', '.hlsl', '.glsl', '.vert', '.frag',
    '.metal', '.wgsl',

    # Smart Contracts
    '.sol', '.vy', '.yul', '.move'
}
