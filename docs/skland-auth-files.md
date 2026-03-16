# Skland Auth Files

本项目后续若继续做森空岛只读验证，本地鉴权输入统一按以下约定存放：

- `uid.txt`
  - 本地忽略文件
  - 只存放一个待验证 UID
- `skland-auth.local.toml`
  - 本地忽略文件
  - 存放真实 `cred`、`token`、可选 `user_id` 或 `access_token`
- `skland-auth.local.example.toml`
  - 可提交模板文件
  - 只定义字段结构，不得填写真实值

字段约定：

- `uid_file`
  - 指向本地 UID 文件路径
- `cred`
  - 森空岛请求头中的 `cred`
- `token`
  - 用于计算 `sign` 的 token
- `user_id`
  - 可选；若已知可减少一次额外查询
- `access_token`
  - 可选；若只有 access token，可后续再换取 `cred` / `token`

使用边界：

- 真实凭据只允许进入被 `.gitignore` 忽略的本地文件。
- `AGENTS.md`、示例模板、文档和提交记录中不得写入真实 UID、`cred`、`token` 或其他个人标识。
- 在未明确需要写代码前，优先通过本地只读脚本验证鉴权是否成功，再决定是否接入仓库命令。
