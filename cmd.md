# Codelens 命令清单

本文记录高频用法。完整参数和默认值以当前版本的 `codelens --help`、`codelens <子命令> --help` 为准。

## 一、默认命令与七个子命令

| 命令 | 用途 | 是否需要 Git 仓库 |
| --- | --- | --- |
| `codelens [路径]` | 一次输出代码统计、健康度和四种成本估算 | 否 |
| `codelens health` | 按复杂度、函数长度、注释率、文件大小、嵌套和重复度评估健康度 | 否 |
| `codelens hotspot` | 找出改动频繁且复杂的风险文件 | 是 |
| `codelens coupling` | 找出经常一起修改的文件 | 是 |
| `codelens diff` | 比较两个 Git 版本或某个版本与当前工作区 | 是 |
| `codelens trend` | 保存和比较 `.codelens/snapshots/` 中的快照 | 否 |
| `codelens estimate` | 使用四种模型估算开发成本、工期和人员 | 否 |
| `codelens mcp` | 为 AI 编程工具提供统计、健康度、热点和变更关联查询 | 部分功能需要 |

MCP 的统计和健康度查询不需要 Git；热点和变更关联查询需要 Git 历史。

## 二、日常常用命令

```bash
codelens .                              # 统计 + 健康度 + 四种成本估算
codelens health . --top 20             # 显示最差的 20 个目录和文件，默认各显示 10 个
codelens health . --health-model v2    # 使用文件优先的 v2 健康模型
codelens hotspot .                     # 分析近 90 天的风险热点
codelens hotspot . --since 30d         # 时间范围支持 30d、4w、6m、1y 或具体日期
codelens hotspot . --functions         # 继续下钻到函数级热点
codelens coupling .                    # 查找经常一起修改的文件，默认排除测试文件
codelens coupling . --for src/api.rs   # 查找通常需要随目标文件一起检查的文件
```

## 三、健康模型

默认使用 v1，以保持历史结果兼容。v2 需要显式选择：

```bash
codelens health . --health-model v1
codelens health . --health-model v2
codelens diff main --health-model v2
```

- v1 使用旧版完整评分流程，范围显示为 `legacy-all`。
- v2 先为文件评分，再汇总项目、语言和目录；支持的语言会把生产代码和内联测试分开。
- v2 的测量覆盖率低于 60% 时，结果标记为 `insufficient-data`，数值只作为估算。
- `--no-dup-scan` 会停止重复度扫描，并从健康分中移除该维度，不会把它当成满分。

## 四、改动前后对比

```bash
# Git 引用对比，不需要提前保存快照
codelens diff develop               # develop 与当前工作区
codelens diff v1.0..v2.0            # 两个版本
codelens diff main...HEAD           # 从 main 与 HEAD 的共同祖先开始比较

# 快照对比，可以跨会话保留
codelens trend --save --label before-refactor
codelens trend                      # 比较最近两份快照
codelens trend --list               # 列出全部快照
codelens trend --compare latest~2 latest
codelens health . --baseline latest
codelens health . --baseline develop
```

## 五、CI 质量门禁

```bash
codelens health . --fail-under B                        # 低于 B 级时失败
codelens health . --fail-under 80                       # 也可以使用 0～100 的分数
codelens health . --baseline main --fail-on-regression  # 只阻止相对基线新增的退步
codelens diff main --fail-on-regression                 # 比较版本并阻止退步
```

`--fail-under` 会检查当前整体结果，已有问题也会计入。`--fail-on-regression` 只检查相对基线新增的退步，例如项目或已有文件降级、下降至少 5 分，或新增文件得到 F。v2 数据不足时，两种门禁都会失败，避免把估算结果误当成通过。

## 六、控制分析范围

下面这些参数适用于默认分析及大多数分析子命令；`mcp` 只接受自身帮助中列出的参数。

```bash
codelens services libs                         # 分析一个或多个目录
codelens . -l go,python                        # 只分析指定语言
codelens . --exclude vendor,dist               # 用逗号分隔的 glob 排除路径
codelens . --include-files 'src/.*'            # 只保留匹配正则的文件
codelens . --exclude-files '.*_generated\.go'  # 排除匹配正则的文件
codelens . --count-as jsp:html                 # 把扩展名归到指定语言
codelens . --no-duplicates                     # 跳过内容完全相同的重复文件
codelens . --no-min-gen                        # 跳过压缩文件和带生成标记的文件
codelens . --no-linguist                       # 忽略 .gitattributes 的 Linguist 规则
codelens . --no-gitignore                      # 不使用 .gitignore 规则
codelens . --no-smart-exclude                  # 不自动排除依赖、构建和缓存目录
codelens . --all                               # 包含默认排除的依赖、构建及 Linguist vendored/generated 文件
```

`-a / --all` 只放开默认的依赖、构建目录智能排除，以及 `.gitattributes` 的 Linguist vendored/generated 排除。它仍遵循 `.gitignore`、`.codelensignore`、显式 include/exclude、语言和行数等过滤，并始终跳过版本控制元数据和二进制文件。

配置和自定义语言：

```bash
codelens . --config path/to/codelens.toml      # 指定配置文件
codelens . --no-config                         # 不加载 .codelens.toml
codelens --list-languages                      # 查看内置语言
codelens --languages-file languages.toml       # 加载自定义语言或内部 DSL
```

## 七、变更关联降噪参数

```bash
codelens coupling . --min-shared 5       # 至少共同出现在 5 个提交中，默认 5
codelens coupling . --min-coupling 30    # 关联度至少 30%，默认 30
codelens coupling . --max-changeset 30   # 忽略修改文件数超过 30 的提交
codelens coupling . --include-tests      # 包含测试文件
```

如果 `--for` 指向测试文件，会自动包含测试文件。关联度表示两个文件共同修改次数除以各自修改次数的平均值；它说明历史相关性，不代表代码调用关系。

## 八、输出格式

```bash
codelens . -f json                       # 机器可读 JSON
codelens . -f csv                        # CSV
codelens . -f markdown                   # Markdown
codelens . -f html                       # HTML
codelens health . -f sarif               # SARIF 2.1.0
codelens health . -f openmetrics         # Prometheus/OpenMetrics
codelens health . -f badge               # shields.io 徽章 JSON
codelens . -f json -O report.json        # 写入文件
codelens . -s                            # 只显示摘要
codelens . -q                            # 静默；配合 -O 时仍写文件
codelens . -v                            # 显示更多信息
```

默认统计报告的明细控制：

```bash
codelens . --by-file --sort code --top 20
codelens . --by-dir --dir-depth 3
codelens . --tokens
codelens . --git-info
```

健康报告会直接显示项目、语言、目录和最差文件，不依赖 `--by-file` 或 `--by-dir`。

## 九、成本估算

```bash
codelens estimate .                                      # 比较全部四种模型
codelens estimate . --model cocomo2                      # 单模型，含语言明细
codelens estimate . --model cocomo-basic
codelens estimate . --model putnam --ck 11000
codelens estimate . --model locomo --locomo-preset large
codelens estimate . --avg-wage 100000                    # 年薪，单位为美元
```

LOCOMO 预设支持 `large`、`medium`、`small` 和 `local`；默认是 `medium`。

## 十、接入 AI 工具

```bash
claude mcp add codelens -- codelens mcp
```

注册后，AI 工具可以查询代码统计、健康度、热点和变更关联。MCP 运行时使用启动目录作为默认项目目录。

当前 MCP 的 `change_coupling` 会把历史中的全部文件和测试文件算进去；CLI 默认排除测试文件，因此两边结果可能不同。

## 十一、使用建议

- 给脚本或 AI 读取时优先使用 `-f json`，再用 `jq` 选择字段。
- 热点控制台的 `Auth` 列会用 `★` 标出知识孤岛，并在表格后显示主要作者和占比。
- JSON 中对应字段是 `knowledge.knowledge_island` 和 `knowledge.ownership`。
- 关联结果为空时，先降低 `--min-shared` 和 `--min-coupling`，不要直接判断文件之间没有关联。
