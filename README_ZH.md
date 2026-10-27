<p align="center"><a href="README.md">English</a> · <strong>简体中文</strong></p>

<p align="center"><img src="docs/assets/readme-hero.png" alt="xhs-cli: revise a note while keeping its ID" width="100%"></p>

# xhs-cli

**在终端读取、发布和修改小红书笔记。** 原生 Rust 实现 HTTP 请求与签名，支持 App 扫码登录和 JSON 输出。命令名为 `xhs`。

[快速开始](#快速开始) · [修改已有笔记](#先发私密再修改) · [完整命令手册](docs/usage.md) · [English](README.md)

- **保留同一条笔记 ID。** 改文案、替换图片或视频、调整可见性；未指定的编辑字段保持原样。
- **覆盖实际素材流程。** 图文、带封面的视频、文档附件和公开视频下载。
- **便于脚本与 Agent 调用。** 搜索、读取、查看自己的笔记、解析话题，输出结构化 JSON。

## 快速开始

需要当前稳定版 Rust 工具链。从源码安装：

```sh
git clone https://github.com/OpenMedia-Lab/xhs-cli.git
cd xhs-cli
cargo install --path . --locked

xhs login --qr
xhs me
xhs search 'Rust'
```

用小红书 App 扫码并在手机确认。加 `--qr-file ./xhs-login.svg` 可导出二维码。不需要 Python 或浏览器自动化进程。也支持[导入 Cookie 和指定会话文件](docs/usage.md)。

## 先发私密，再修改

准备自己的 `cover.png` 与 UTF-8 正文文件 `body.txt`。**下面会在账号上创建一条私密笔记：**

```sh
xhs publish --title '分享一个值得了解的项目' --desc-file body.txt \
  --image cover.png --visibility private
```

保留返回的笔记 ID，后续直接更新原笔记：

```sh
xhs edit NOTE_ID --title '更清楚的标题' --desc-file revised-body.txt
xhs edit NOTE_ID --image cover-v2.png --image detail-v2.png
xhs detail NOTE_ID
```

未指定图片和可见性时保留原值；指定图片会替换整组图片。私密笔记已经保存在平台上，不是本地草稿。

## 按任务选择命令

| 任务 | 入口 |
|---|---|
| 搜索并阅读 | `xhs search '关键词'`，再用 `xhs read 'SIGNED_NOTE_URL'` |
| 查看自己的账号与笔记 | `xhs me`、`xhs my-notes`、`xhs detail NOTE_ID` |
| 绑定原生话题 | `xhs topics '关键词'`，发布时重复传入 `--topic '精确名称'` |
| 发布或替换视频 | [视频指南](docs/video-publishing.md)：上传 → 预览 → 单次提交 → 回读 |
| 上传文档、下载视频 | [完整命令手册](docs/usage.md) |

读取他人的笔记时，使用搜索结果带访问令牌的完整链接；裸 ID 不一定可用。原生话题实体与正文可见的 `#话题[话题]#` 标记需要分别处理。视频发布使用持久化回执；结果不确定时用 `check-publish` 回读核对，不重复发帖。

## 自动化前需要了解

平台网页接口可能变化，账号校验、审核和频率限制依然有效。CLI 不承诺无限制发布或推荐流量。已验证的行为与边界见 [VALIDATION.md](VALIDATION.md)。

默认会话路径为 `~/.xiaohongshu-cli/cookies.json`；会话文件和发布回执不要提交到 Git。扫码语义见[登录验证说明](docs/qr-login-verification.md)。写请求结果不确定时，先查看自己的笔记再决定下一步。

## 参与贡献

提交问题时附上命令、脱敏错误 JSON、预期行为和版本，别附 Cookie 或签名头。修改代码后先运行：

```sh
cargo fmt --all -- --check
cargo test --locked
```

## 一起使用

[postdeck](https://github.com/OpenMedia-Lab/postdeck) 负责文章加工与图卡；平台操作交给独立 CLI： [xhs-cli](https://github.com/OpenMedia-Lab/xhs-cli), [zhihu-cli](https://github.com/OpenMedia-Lab/zhihu-cli), [x-cli](https://github.com/OpenMedia-Lab/x-cli). 更多项目见 [OpenMedia-Lab](https://github.com/OpenMedia-Lab).

## 许可与来源

[Apache-2.0](LICENSE)，部分组件使用独立许可。Rust 实现参考 [jackwener/xiaohongshu-cli](https://github.com/jackwener/xiaohongshu-cli)；归属与许可见 [NOTICE](NOTICE)、[LICENSE-xhshow](LICENSE-xhshow)。
