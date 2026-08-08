# Codelens 发布流程

本文档描述 codelens 的完整发布流程，遵循 Rust 生态最佳实践。

## 目录

1. [发布前准备](#发布前准备)
2. [版本管理](#版本管理)
3. [发布步骤](#发布步骤)
4. [自动化发布](#自动化发布)
5. [发布渠道](#发布渠道)
6. [回滚流程](#回滚流程)

---

## 发布前准备

### 1. 安装必要工具

```bash
# 版本管理工具
cargo install cargo-release

# 安全审计工具
cargo install cargo-audit
cargo install cargo-deny

# 跨平台编译工具（可选）
cargo install cross

# 二进制大小优化分析（可选）
cargo install cargo-bloat
```

### 2. 配置 crates.io

```bash
# 登录 crates.io（需要 API token）
cargo login

# 验证登录状态
cargo owner --list
```

### 3. 配置 GitHub Secrets

在仓库 Settings → Secrets and variables → Actions 中添加：

| Secret 名称 | 用途 | 获取方式 |
|------------|------|---------|
| `CARGO_REGISTRY_TOKEN` | crates.io 发布 | https://crates.io/settings/tokens |
| `HOMEBREW_TAP_TOKEN` | Homebrew 更新 | GitHub PAT with repo scope |

---

## 版本管理

### 语义化版本

遵循 [SemVer 2.0.0](https://semver.org/)：

- **MAJOR** (x.0.0): 不兼容的 API 变更
- **MINOR** (0.x.0): 向后兼容的功能新增
- **PATCH** (0.0.x): 向后兼容的问题修复

### 预发布版本

```
0.1.0-alpha.1  # 内测版本
0.1.0-beta.1   # 公测版本
0.1.0-rc.1     # 候选发布版本
0.1.0          # 正式版本
```

### 多语言分支 Tag 命名

本仓库包含多个语言实现，各分支使用独立的 tag 后缀：

| 分支 | Tag 格式 | 示例 |
|------|---------|------|
| `rust` | `v{version}-rust` | `v0.1.0-rust` |
| `python` | `v{version}-python` | `v1.0.0-python` |
| `go` | `v{version}-go` | `v0.1.0-go` |

### 版本同步

workspace 中所有 crate 共享版本号，在根 `Cargo.toml` 中定义：

```toml
[workspace.package]
version = "0.1.0"
```

---

## 发布步骤

> **推荐统一使用 cargo release 发版**（见下文「使用 cargo-release（推荐）」小节）。
> cargo release 会自动 bump workspace 版本号，并按 release.toml 的替换规则更新
> CHANGELOG.md 的 `[Unreleased]`、同步 README.md 和 `.pre-commit-hooks.yaml` 中的
> `rev:` 示例、创建 tag，一条命令完成。手动流程仅作兜底，所有替换都必须人工逐项
> 完成，容易遗漏。

### 手动发布流程（兜底）

#### 1. 检查清单

```bash
# 确保在正确的分支
git checkout rust
git pull origin rust

# 运行完整测试
cargo test --all-features

# 代码格式检查
cargo fmt --all -- --check

# Lint 检查
cargo clippy --all-targets --all-features -- -D warnings

# 安全审计
cargo audit
cargo deny check

# 文档构建测试
cargo doc --no-deps --all-features
```

#### 2. 更新版本

以下文件**必须逐一手动更新**。走 cargo release 时版本号由命令本身 bump，其余替换
由 `crates/codelens/release.toml` 的 `pre-release-replacements` 自动完成；手动发版没有任何机制兜底
（v0.1.6 手动发版时这些替换规则完全没有生效，全靠人工记得修改才没出错）：

- [ ] 根 `Cargo.toml`：`[workspace.package]` 中的 `version`
- [ ] `CHANGELOG.md`：将 `[Unreleased]` 改为 `[X.Y.Z] - YYYY-MM-DD`，并在
  `<!-- next-header -->` 标记下方补回一个空的 `## [Unreleased]` 段
  （cargo release 会自动做这两步，标记行请勿删除）
- [ ] `README.md`：pre-commit 示例中的 `rev: vX.Y.Z-rust`
- [ ] `.pre-commit-hooks.yaml`：注释示例中的 `rev: vX.Y.Z-rust`

```bash
# 编辑 Cargo.toml 中的版本号
vim Cargo.toml

# 更新 CHANGELOG.md
vim CHANGELOG.md

# 更新 README.md 与 .pre-commit-hooks.yaml 中的 rev:
vim README.md .pre-commit-hooks.yaml

# 更新 Cargo.lock
cargo check
```

#### 3. 提交版本变更

```bash
git add Cargo.toml Cargo.lock CHANGELOG.md README.md .pre-commit-hooks.yaml
git commit -m "chore: bump version to 0.1.0"
```

#### 4. 创建 Tag 并推送

```bash
# 创建带注释的 tag（Rust 版本使用 -rust 后缀）
git tag -a v0.1.0-rust -m "Release v0.1.0 (Rust)"

# 推送 tag（触发自动发布）
git push origin v0.1.0-rust
```

### 使用 cargo-release（推荐）

cargo release 会按 release.toml 的配置（workspace 级在仓库根，tag 和文件替换在
`crates/codelens/release.toml`）自动完成上述全部手动步骤：bump workspace
版本号、替换 CHANGELOG.md 的 `[Unreleased]`、同步 README.md 和
`.pre-commit-hooks.yaml` 中的 `rev:`、提交并创建 tag：

```bash
# 预览发布（不实际执行）
cargo release patch --dry-run

# 执行发布
cargo release patch --execute

# 发布 minor 版本
cargo release minor --execute

# 发布 major 版本
cargo release major --execute
```

---

## 自动化发布

推送 tag 后，GitHub Actions 自动执行：

### CI 流程 (ci.yml)

1. **Check** - 编译检查
2. **Format** - 代码格式检查
3. **Clippy** - Lint 检查
4. **Test** - 跨平台测试 (Linux/macOS/Windows)
5. **MSRV** - 最低 Rust 版本兼容性
6. **Security** - 安全审计
7. **Docs** - 文档构建

### Release 流程 (release.yml)

1. **Create Release** - 创建 GitHub Draft Release
2. **Build** - 构建多平台二进制并上传到 Draft Release
   - `x86_64-unknown-linux-gnu`
   - `x86_64-unknown-linux-musl`
   - `aarch64-unknown-linux-gnu`
   - `x86_64-apple-darwin`
   - `aarch64-apple-darwin`
   - `x86_64-pc-windows-msvc`
3. **Publish Release** - Build 全部完成后自动将 Draft Release 转正（正式发布）
4. **Publish crates** - 发布到 crates.io
5. **Homebrew** - 更新 Homebrew formula（依赖已转正的 Release：Draft 状态的资产
   无法通过公开链接下载，转正后才能下载资产并计算校验和）

> 兜底：若 publish-release job 失败，Release 会停留在 Draft 状态，Homebrew formula
> 也不会更新。此时在 GitHub Releases 页面手动将 Draft 发布（Publish release），
> 然后在 Actions 页面 re-run homebrew job；仍失败则参考[常见问题](#常见问题)手动
> 更新 formula。

---

## 发布渠道

### 1. GitHub Releases

自动生成，包含：
- 各平台预编译二进制
- 自动生成的 Release Notes
- 源码压缩包

### 2. crates.io

```bash
# 手动发布（如果自动发布失败）
cargo publish -p codelens-core
sleep 30  # 等待 crates.io 索引更新
cargo publish -p codelens
```

安装方式：
```bash
cargo install codelens
```

### 3. Homebrew (macOS/Linux)

需要维护 [homebrew-tap](https://github.com/DropFan/homebrew-tap) 仓库。

Formula 模板 (`codelens.rb`):
```ruby
class Codelens < Formula
  desc "High performance code analysis tool — stats, health scores, hotspots, and trends"
  homepage "https://github.com/DropFan/codelens"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/DropFan/codelens/releases/download/v#{version}/codelens-#{version}-aarch64-apple-darwin.tar.gz"
      sha256 "XXXXXXX"
    end
    on_intel do
      url "https://github.com/DropFan/codelens/releases/download/v#{version}/codelens-#{version}-x86_64-apple-darwin.tar.gz"
      sha256 "XXXXXXX"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/DropFan/codelens/releases/download/v#{version}/codelens-#{version}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "XXXXXXX"
    end
    on_intel do
      url "https://github.com/DropFan/codelens/releases/download/v#{version}/codelens-#{version}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "XXXXXXX"
    end
  end

  def install
    bin.install "codelens"
  end

  test do
    system "#{bin}/codelens", "--version"
  end
end
```

安装方式：
```bash
brew tap DropFan/tap
brew install codelens
```

### 4. 其他渠道（可选扩展）

- **Scoop** (Windows): 创建 scoop bucket
- **AUR** (Arch Linux): 创建 PKGBUILD
- **Nix**: 添加到 nixpkgs
- **Docker**: 发布 Docker 镜像

---

## 回滚流程

### 撤销 crates.io 发布

crates.io 不支持删除已发布版本，但可以 yank：

```bash
# Yank 有问题的版本（阻止新项目依赖）
cargo yank --version 0.1.0 codelens
cargo yank --version 0.1.0 codelens-core

# 取消 yank
cargo yank --undo --version 0.1.0 codelens
```

### 撤销 GitHub Release

```bash
# 删除 tag
git tag -d v0.1.0-rust
git push origin :refs/tags/v0.1.0-rust

# 在 GitHub 网页上删除 Release
```

### 发布修复版本

```bash
# 修复问题后发布 patch 版本
git checkout rust
# ... 修复代码 ...
cargo release patch --execute
```

---

## 发布检查清单

发布前确认：

- [ ] 所有测试通过
- [ ] CHANGELOG.md 已更新
- [ ] 版本号已更新
- [ ] README.md 与 `.pre-commit-hooks.yaml` 中的 `rev:` 已指向新版本
      （cargo release 自动处理，手动发版必须检查）
- [ ] 文档已更新
- [ ] 无安全漏洞 (`cargo audit`)
- [ ] CI 全部通过

发布后确认：

- [ ] GitHub Release 已发布（Draft 由 publish-release job 自动转正）
- [ ] 各平台二进制可下载
- [ ] crates.io 发布成功
- [ ] `cargo install codelens` 可正常安装
- [ ] Homebrew formula 更新（如适用）

---

## 常见问题

### Q: crates.io 发布失败？

1. 检查 `CARGO_REGISTRY_TOKEN` 是否有效
2. 确认 crate 名称未被占用
3. 检查 Cargo.toml 元数据完整性

### Q: 跨平台构建失败？

1. 检查 `cross` 是否正确安装
2. 确认 Docker 正常运行（cross 依赖 Docker）
3. 查看具体错误日志

### Q: Homebrew 更新失败？

1. 检查 `HOMEBREW_TAP_TOKEN` 权限
2. 确认 tap 仓库存在
3. 手动更新 formula 并创建 PR
