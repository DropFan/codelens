"""排除目录配置"""

from typing import Dict, Set

# 基础排除目录（所有项目通用）
BASE_EXCLUDE_DIRS: Set[str] = {
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
LANGUAGE_EXCLUDE_DIRS: Dict[str, Set[str]] = {
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

# 为了向后兼容，合并所有语言的排除目录
DEFAULT_EXCLUDE_DIRS: Set[str] = BASE_EXCLUDE_DIRS.copy()
for _lang_excludes in LANGUAGE_EXCLUDE_DIRS.values():
    DEFAULT_EXCLUDE_DIRS.update(_lang_excludes)
