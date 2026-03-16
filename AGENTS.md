# AGENTS.md

## 1. 项目使命

构建一个面向《明日方舟》国服的 Windows 10/11 桌面程序，用于：
- 接入 MuMu 模拟器；
- 读取并维护本地仓库数据库；
- 读取并维护本地干员与养成数据库；
- 当扫描到仓库/干员状态与数据库不一致时自动更新数据库，并保留审计历史；
- 每日同步外部养成数据、干员数据、掉率数据、活动公告；
- 基于仓库、box、养成目标、活动窗口、药剂过期时间，给出并可执行资源规划；
- 在不动用抽卡资源的前提下，最大化快速精二/专精能力；
- 基于 box 与基建技能给出基建轮班表；
- 可选接入 DeepSeek API 做解释、总结、容错解析，但 DeepSeek 不能成为本地数据库写入的唯一依据。

本程序是“看号 + 记账 + 规划 + 安全自动化”工具，不是纯 OCR 工具，也不是纯脚本工具。

## 2. 冻结约束（不可违背）

- 编程语言：Rust
- 工具链：nightly-2025-07-12
- 目标系统：Windows 10/11 x64
- GUI：eframe/egui
- 服务器默认：国服（CN）
- 默认游戏时区：Asia/Shanghai
- 不允许使用以下方式作为主方案：
  - 进程注入
  - DLL 注入
  - 内存读写
  - 协议逆向后的私有接口
  - 修改游戏客户端文件
- 与 MuMu/游戏的交互主通道必须是：
  - ADB 截图
  - ADB 输入（tap/swipe/keyevent/text）
  - 本地视觉识别
- 程序必须是“本地优先”：没有 DeepSeek API 时，核心功能仍可用。
- 程序必须保留“人工校正”入口，不能把低置信度识别结果直接写死。
- 程序必须维护历史快照，不允许只保留最终态。
- 每次编码步骤结束后，必须更新本文件中的：
  - 新需求
  - 新重要记忆
  - 已完成内容
  - 当前下一步

## 3. 产品定义

### 3.1 “看号”在本项目中的定义

“看号”= 自动采集 + 本地结构化存档 + 差异更新 + 资源规划 + 执行建议。

### 3.2 必做能力

1. 仓库全量记录
2. 干员全量记录
3. 养成程度记录
4. 差异检测与数据库更新
5. 每日同步外部资料
6. 关卡收益推荐
7. 稀缺材料优先补足
8. 体力药过期与活动开始提醒
9. 小材料在满足底线后自动/半自动合成高阶材料
10. 基建轮班优化
11. 区分白嫖资源 / 消耗型养成资源 / 抽卡资源
12. 保障“快速精二一名干员”和“快速专精一个技能”的资源底线

### 3.3 非目标（至少 v1 不做）

- 多服务器混合支持
- iOS 支持
- 内存 Hook
- 漏洞式自动化
- 自动抽卡
- 自动购买氪金商品
- 没有人工确认的高风险资源消耗
- 靠 LLM 直接“猜测”数据库最终值

## 4. 关键设计决策（先定再做）

### 4.1 MuMu 接入方案

采用“外部 ADB 可执行文件 + 端口自动发现 + 手动覆盖”模式：
- 默认自动尝试连接：
  - 127.0.0.1:7555
  - 16384 + 32 * n（n 从 0 开始）
  - 如连接失败，提供手动端口输入
- 支持单开与多开
- 所有设备操作统一经 `DeviceSession` 抽象完成

不在 v1 直接实现原生 ADB 协议栈，优先 shell-out 到 adb.exe / adb_server.exe。

### 4.2 外部数据源分工

- PRTS：干员、道具、养成材料、基建技能、关卡/掉落静态信息、制作配方、页面变更锚点
- 官方公告：活动开始时间、维护、活动开放窗口
- Penguin Stats：掉率统计、阶段性掉率矩阵
- DeepSeek：说明、解释、低置信度辅助解析、规划总结（仅辅助）

### 4.3 识别策略

采用“规则导航 + 模板识别 + OCR + 置信度审计”方案：
- 不能用纯 OCR 硬扫整屏
- 必须针对页面建立状态机
- 必须维护 ROI（region of interest）与页面模板
- 必须为每项识别打分（confidence）
- 低置信度进入人工复核队列

### 4.4 本地数据库策略

采用 SQLite，本地单文件数据库，开启 WAL。
必须有：
- schema migrations
- 快照表
- 当前态表
- 差异表
- 原始识别产物表
- 外部数据缓存表
- 计划结果表
- 提醒表

### 4.5 LLM 使用边界

DeepSeek 仅用于：
- 自然语言总结
- 函数调用式规划解释
- 低置信度视觉结果辅助判读
- 生成用户提示文本

DeepSeek 不可直接：
- 决定库存数量最终写入
- 决定干员养成最终写入
- 消耗受保护资源
- 覆盖规则引擎的硬约束

## 5. 资源策略（强约束）

### 5.1 资源分类

必须实现以下资源分类枚举，并允许用户覆写：

- `HardProtected`：硬保护，绝不自动消耗
- `SoftProtected`：软保护，默认不自动消耗，需显式允许
- `ReusableFree`：可重复利用/白嫖资源，可自动调度
- `FarmableCore`：可刷可补的核心养成资源
- `ExpiringStrategic`：会过期且需要时机管理的资源
- `Convertible`：可合成/可逆向规划的材料
- `ManualOnly`：只允许人工操作

### 5.2 默认分类

默认必须这样处理：
- 合成玉：`HardProtected`
- 源石 / 至纯源石：`HardProtected`
- 寻访凭证 / 十连券 / 限定抽卡资源：`HardProtected`
- 高级凭证、重要票据类：`SoftProtected`
- 无人机：`ReusableFree`
- 龙门币：`FarmableCore`
- 作战记录：`FarmableCore`
- 体力药：`ExpiringStrategic`
- 养成材料：`Convertible` 或 `FarmableCore`
- 芯片、双芯片：`FarmableCore`
- 技巧概要：`FarmableCore`

### 5.3 无人机策略

无人机默认允许自动使用，但必须满足：
- 不触碰抽卡资源
- 不为了补芯片而被迫加速临时制造
- 以“当前瓶颈资源”为目标进行加速
- 默认优先级：
  1. 保障紧急目标的龙门币/作战记录缺口
  2. 保障订单收益
  3. 保障当前主线缺口对应的产线

### 5.4 体力药策略

体力药必须：
- 记录精确过期时间
- 基于官方活动公告推断是否值得保留
- 在活动已被官方确认且开始时间进入窗口时，主动提醒“哪些药应该留到活动”
- 不允许基于泄露/推测时间线做硬提醒
- 若活动尚未官宣，只能提示“存在潜在活动窗口，不做自动保留决策”

## 6. 快速精二 / 快速专精底线模型

必须实现“底线配置（Floor Profiles）”。

### 6.1 最低必须支持的底线配置

- `EmergencyE2`
- `EmergencyMastery`

### 6.2 Floor Profile 的定义

Floor Profile = 一组用户想要始终留在仓库中的最低资源组合。
Floor Profile 由以下组成：
- 目标类型：精二 / 专精 / 模组 / 自定义
- 候选干员集合
- 候选技能集合（用于专精）
- 对应材料、龙门币、经验、芯片、技巧书、模组材料等需求
- 允许合成与否
- 允许使用无人机与否（默认否）
- 允许动用软保护资源与否（默认否）

### 6.3 默认行为

若用户没有明确配置候选干员：
- `EmergencyE2` 默认针对“已拥有且被标星/置顶的干员集合”
- 若仍为空，则默认选择“已拥有但未精二的最高优先级 6★/5★ 干员”
- `EmergencyMastery` 默认针对“已拥有且被标星/置顶的技能集合”
- 若仍为空，则默认选择“已拥有且最接近专精、收益最高的技能”

注意：
- 不把“任意干员”当作默认目标
- 不采用无法落地的“全职业全满配芯片全覆盖”作为默认底线

## 7. 仓库与干员采集范围

### 7.1 仓库记录最小字段

每个物品至少记录：
- item_id
- 中文名
- 类型
- 稀有度/层级
- 当前数量
- 数据来源（识别/手工/同步）
- 识别置信度
- 最后确认时间
- 最近一次变更来源
- 是否受保护
- 是否可合成
- 是否参与底线计算

### 7.2 干员记录最小字段

每个干员至少记录：
- operator_id
- 中文名
- 是否持有
- 稀有度
- 职业 / 分支
- 精英阶段
- 当前等级
- 技能总等级
- 各技能专精等级
- 模组状态与等级
- 是否标星 / 是否纳入紧急目标
- 最后扫描时间
- 识别置信度

v1 可选字段（不是首个阻塞项）：
- 潜能
- 信赖
- 时装
- 悖论模拟 / 密录完成状态

## 8. 基建轮班问题建模

### 8.1 v1 输入

v1 不强制自动识别全部基建房间布局。
必须允许用户在设置中手工配置：
- 基建布局（243 / 252 / 153 / 自定义）
- 房间等级
- 可用房间类型
- 是否启用会客室/训练室/加工站策略偏好
- 用户偏好的生产目标（龙门币 / 经验 / 平衡）

### 8.2 轮班求解目标

轮班表求解必须考虑：
- box 内已拥有干员
- 基建技能
- 房间类型与槽位数
- 干员疲劳恢复与上岗时长
- 订单、制造、会客室、训练室、加工站的目标权重
- 用户当前资源缺口（例如龙门币缺还是经验缺）
- 不重复占用同一干员
- 当前时间与下次收菜/换班窗口

### 8.3 求解策略

先做：
- 候选队伍生成
- 房间局部评分
- 全局不冲突分配
- 疲劳轮换模拟
- Beam Search + Branch and Bound + Local Improvement

不要在第一版就引入复杂外部求解器。
先保证结果稳定、可解释、可复现。

## 9. 小材料合成策略

加工站合成必须满足以下全部条件才允许自动执行：
- 高阶材料存在明确缺口
- 低阶材料扣除后仍高于对应 Floor
- 合成不会导致“紧急精二/专精”底线破坏
- 不会动用硬保护资源
- 不会动用默认禁止的软保护资源
- 合成收益在计划模型中优于保持低阶库存
- 必须能输出“为什么合成、合成多少、合成后还剩多少”的解释

默认：
- 自动执行前需用户确认
- 支持“只建议不执行”模式
- 支持“按计划一键执行”模式

## 10. 外部数据同步策略

### 10.1 PRTS 同步

必须优先走 API，不直接硬爬页面 HTML。
需要同步/校验的核心数据至少包括：
- 干员基础资料
- 干员养成需求
- 基建技能
- 道具资料
- 关卡/材料静态映射
- 制作配方
- 页面版本锚点（revision / oldid / 更新时间）

需要实现：
- 表结构/字段发现与适配
- 失败重试
- 本地缓存
- 源数据版本记录
- 校验失败后的降级策略

### 10.2 官方活动同步

必须将官方公告作为“活动开始时间”的第一可信源。
同步后至少记录：
- 标题
- 活动类型
- 公告发布时间
- 活动开始时间
- 活动结束时间（若可得）
- 来源 URL
- 是否已确认

### 10.3 Penguin Stats 同步

至少同步：
- result matrix
- stage_id / item_id 映射
- 时间区间
- 样本数
- 掉落总量

规划时必须考虑：
- 样本量不足时降权
- 活动关卡关闭时不可推荐
- 已关闭 zone/stage 不得推荐给当前自动化执行层

### 10.4 DeepSeek 同步/调用

DeepSeek 配置必须可选：
- API Key 走系统安全存储或环境变量
- 未配置时不阻塞主程序
- 所有请求必须结构化、可审计
- LLM 输出如用于程序流程，必须使用严格 JSON 结构并经过本地验证

## 11. 数据源优先级

写入本地“拥有状态/库存状态”的优先级：
1. 用户手工确认
2. 高置信度本地识别
3. 中置信度识别 + 人工确认
4. 历史已确认状态
5. LLM 辅助推断（不得直接落最终态）

写入本地“外部资料定义”的优先级：
1. 官方公告（活动时间）
2. PRTS
3. Penguin Stats（掉率）
4. 手工修正
5. LLM 生成说明（不得作为定义源）

## 12. 架构与仓库布局

必须使用 Cargo Workspace。

推荐固定目录如下：

- `apps/akbox-desktop`
  - eframe/egui GUI 程序
- `apps/akbox-cli`
  - 命令行入口：sync / scan / plan / debug
- `crates/akbox-core`
  - 领域模型、规则引擎、规划器、差异逻辑
- `crates/akbox-data`
  - SQLite、migration、repository、外部数据源客户端
- `crates/akbox-device`
  - ADB、MuMu 发现、截图、输入、页面状态机、OCR、模板匹配
- `crates/akbox-testkit`
  - 测试夹具、golden images、mock data sources
- `assets/templates`
  - 页面模板、图标指纹、ROI 配置
- `assets/golden`
  - 识别回归测试样本
- `migrations`
  - SQLite migration SQL
- `docs`
  - 设计说明、字段字典、扫描流程图

## 13. 主要模块职责

### 13.1 `akbox-core`

负责：
- 领域实体
- 资源分类策略
- Floor Profiles
- 差异计算
- 合成规划
- 关卡推荐
- 基建排班
- 提醒生成
- 审计事件模型

### 13.2 `akbox-data`

负责：
- SQLite 连接与 migration
- repository 层
- PRTS 客户端
- 官方公告客户端
- Penguin Stats 客户端
- DeepSeek 客户端
- 缓存
- 同步任务编排

### 13.3 `akbox-device`

负责：
- ADB 可执行文件发现
- MuMu 端口发现
- 设备连接与重连
- 截图
- 输入动作
- 页面状态机
- OCR
- 图标匹配
- 识别置信度计算
- 自动化动作执行

### 13.4 `akbox-desktop`

负责：
- 页面导航
- 仪表盘
- 扫描与同步控制
- 识别结果复核
- 计划展示
- 基建轮班展示
- 提醒展示
- 设置页

### 13.5 `akbox-cli`

负责：
- 定时任务可调用入口
- 开发调试入口
- headless sync / scan / export
- 给 Windows 任务计划程序使用

## 14. 推荐依赖（先按这个选型，不要反复摇摆）

- GUI：
  - `eframe`
  - `egui`
  - `egui_extras`
- 异步：
  - `tokio`
- HTTP：
  - `reqwest` + `rustls`
- 序列化：
  - `serde`
  - `serde_json`
  - `toml`
- 错误处理：
  - `thiserror`
  - `anyhow`
- 日志：
  - `tracing`
  - `tracing-subscriber`
- 数据库：
  - `rusqlite`
  - `rusqlite_migration`
- 时间：
  - `time`
- 图像处理：
  - `image`
  - `imageproc`
  - `fast_image_resize`
- 并行：
  - `rayon`
- Windows OCR：
  - `windows`
- 哈希/指纹：
  - `image_hasher` 或等价方案
- 标识：
  - `uuid`

不要在没有必要的情况下引入：
- WebView
- Electron
- Python 运行时
- 巨型外部求解器
- 重型 ORM

## 15. 识别与自动化技术方案

### 15.1 页面状态机

必须先实现页面状态机，再做大规模识别。
每个页面定义：
- `page_id`
- 页面进入动作
- 页面确认特征
- ROI 集合
- 支持的读取字段
- 支持的执行动作
- 失败恢复动作

### 15.2 OCR 策略

OCR 主要用于：
- 数字数量
- 等级
- 技能等级
- 时间
- 短文本标签

图标/立绘识别主要用：
- 模板匹配
- 指纹哈希
- 页面局部上下文

### 15.3 置信度

所有识别结果必须带 confidence。
建议阈值：
- `>= 0.98`：可自动确认
- `0.90 ~ 0.98`：自动入库但标记“待复核”
- `< 0.90`：进入人工确认队列，不得直接覆盖已确认值

阈值可以配置，但必须有默认值。

### 15.4 自动化执行安全

默认所有高影响动作均需要确认：
- 加工站批量合成
- 无人机批量使用
- 基建批量换班
- 批量开图/连续作战
- 使用体力药

支持三种模式：
- 建议模式：只给方案，不执行
- 半自动模式：逐步确认
- 自动模式：执行已确认计划模板

默认启用建议模式。

## 16. 数据库最低表设计

至少建立以下表（可扩展，不可缺核心语义）：

- `app_meta`
- `sync_source_state`
- `raw_source_cache`
- `external_operator_def`
- `external_operator_growth`
- `external_operator_building_skill`
- `external_item_def`
- `external_recipe`
- `external_stage_def`
- `external_drop_matrix`
- `external_event_notice`
- `inventory_snapshot`
- `inventory_item_state`
- `operator_snapshot`
- `operator_state`
- `scan_artifact`
- `recognition_review_queue`
- `resource_policy`
- `floor_profile`
- `floor_profile_member`
- `planner_run`
- `planner_recommendation`
- `base_layout_config`
- `base_shift_plan`
- `alert`
- `audit_log`

要求：
- 当前态与历史快照分离
- 任何覆盖更新都写审计日志
- 外部定义与用户拥有状态分离

## 17. UI 最小页面清单

必须至少有以下页面：

1. 仪表盘
   - 今日同步状态
   - 今日扫描状态
   - 当前资源警报
   - 今日推荐副本
   - 今日基建提醒

2. 设备页
   - ADB 路径
   - MuMu 设备发现
   - 连接状态
   - 实时截图预览

3. 同步页
   - PRTS 同步
   - 官方公告同步
   - Penguin 同步
   - DeepSeek 配置测试

4. 扫描页
   - 仓库扫描
   - 干员扫描
   - 低置信度复核
   - 差异对比

5. 仓库页
   - 当前库存
   - 历史变化
   - Floor 差距

6. 干员页
   - box 总览
   - 单干员详情
   - 养成缺口

7. 规划页
   - 紧急精二计划
   - 紧急专精计划
   - 今日刷图建议
   - 合成建议

8. 基建页
   - 布局设置
   - 轮班方案
   - 无人机建议

9. 提醒页
   - 体力药提醒
   - 活动提醒
   - 资源底线提醒
   - 同步失败提醒

10. 设置页
    - 时区
    - 资源保护策略
    - Floor 配置
    - 自动化模式

## 18. 有序开发路线（必须按阶段推进）

### 阶段 0：工程初始化

完成条件：
- 创建 Cargo Workspace
- 建立 `rust-toolchain.toml`
- 建立 `cargo fmt` / `clippy` / `test` 基础命令
- 建立基础目录结构
- 建立本文件并写入初始内容
- GUI 可启动并显示空壳主窗口
- CLI 可运行 `--help`

### 阶段 1：配置与日志

完成条件：
- 读取配置文件
- 支持 ADB 路径配置
- 支持游戏时区配置
- 支持日志输出到文件
- 支持 debug 模式下导出截图与识别结果

### 阶段 2：SQLite 与 migration

完成条件：
- 建立数据库
- 建立 migration 系统
- 建立所有核心表
- 建立 repository 基础接口
- 建立 audit_log 写入能力

### 阶段 3：外部数据同步骨架

完成条件：
- PRTS 客户端打通
- 官方公告客户端打通
- Penguin 客户端打通
- 同步结果可写入数据库
- 同步失败可告警
- 原始响应可缓存

### 阶段 4：MuMu/ADB 接入

完成条件：
- 自动发现设备
- 手动指定端口
- 连接/重连
- 截图
- tap/swipe/keyevent
- 截图预览页可用

### 阶段 5：视觉基础设施

完成条件：
- ROI 配置机制
- 模板匹配
- OCR 封装
- confidence 计算
- scan_artifact 入库
- review_queue 入库

### 阶段 6：仓库扫描 v1

完成条件：
- 可从仓库页面扫描物品数量
- 能正确翻页
- 能检测重复页/结束页
- 能生成 inventory snapshot
- 能将当前态与历史态做 diff
- 可在 UI 中人工修正

### 阶段 7：干员扫描 v1

完成条件：
- 能从干员列表识别已拥有干员
- 能进入单干员详情页
- 能识别精英阶段、等级、技能等级、专精、模组状态
- 能入库 operator snapshot / current state
- 能展示差异结果
- 低置信度支持人工复核

### 阶段 8：Floor / 规划引擎

完成条件：
- 支持 EmergencyE2
- 支持 EmergencyMastery
- 能根据当前库存和 box 计算缺口
- 能输出“缺什么、差多少、优先补什么”

### 阶段 9：关卡收益推荐

完成条件：
- 接入 Penguin 掉率
- 结合当前缺口排序
- 给出今日最优刷图建议
- 解释推荐原因
- 关闭关卡不得推荐

### 阶段 10：加工站合成规划

完成条件：
- 能识别何时应从小材料合成大材料
- 不破坏低阶 Floor
- 能输出解释
- 支持建议模式
- 支持执行模式（需确认）

### 阶段 11：基建轮班规划

完成条件：
- 支持手动配置基建布局
- 支持根据 box 生成轮班表
- 支持疲劳模拟
- 支持无人机建议
- 能展示方案评分与理由

### 阶段 12：活动提醒与体力药窗口

完成条件：
- 官方活动开始时间可入库
- 体力药过期时间可入库
- 能判断哪些药应留、哪些药应消耗
- 能生成提醒
- 时间展示必须带绝对日期和时区

### 阶段 13：DeepSeek 可选增强

完成条件：
- 支持 API Key 配置
- 支持函数调用式 JSON 输出
- 支持生成规划摘要/解释
- 支持低置信度辅助解析，但不得绕开本地校验

### 阶段 14：收尾与可交付

完成条件：
- 全流程 smoke test
- 关键识别页面 golden tests
- CLI 支持定时同步
- Windows 打包说明
- 数据备份/恢复
- 文档补齐

## 19. 测试策略（必须执行）

### 19.1 单元测试

必须覆盖：
- 资源分类规则
- Floor 计算
- 缺口计算
- 掉率推荐排序
- 合成决策
- 基建局部评分

### 19.2 集成测试

必须覆盖：
- 数据库 migration
- 外部同步写库
- ADB 命令构造
- 页面状态机流转
- 仓库扫描流程
- 干员扫描流程

### 19.3 Golden Tests

必须建立 golden images：
- 仓库页
- 干员列表页
- 干员详情页
- 技能页
- 模组页
- 基建页关键页面

### 19.4 人工验收

必须提供人工验收清单：
- 第一次接入 MuMu 是否成功
- 第一次同步是否成功
- 第一次仓库扫描是否成功
- 第一次干员扫描是否成功
- 推荐副本是否合理
- 合成建议是否合理
- 基建轮班是否可解释

## 20. 性能与稳定性要求

- 不能因为一次同步失败损坏数据库
- 不能因为一次 OCR 失败覆盖已确认值
- 扫描过程必须可恢复
- 自动化执行必须可中断
- 所有重要操作必须写日志
- UI 不允许长时间无响应；耗时任务必须后台执行并回传进度

## 21. Codex 工作协议（强制）

每次开始编码前，必须执行：

1. 先完整阅读本文件
2. 读取“当前待办”
3. 如果用户提出了新要求，先更新本文件，再写代码
4. 只做一个最小闭环阶段，不做大爆炸式改动
5. 完成后运行：
   - `cargo fmt`
   - `cargo clippy --workspace --all-targets`
   - `cargo test --workspace`
6. 把结果写回本文件
7. 如果有未完成项，写进“下一步”
8. 如果做了新的关键设计决策，写进“重要记忆”
9. 如果旧要求被新要求替代，不得删除，只能标记“已被替代”并说明原因

## 22. AGENTS.md 更新格式（必须追加，不得静默改没）

每次完成一个编码步骤后，按以下结构追加一条记录：

### 变更记录模板

- 日期时间：
- 阶段：
- 新需求：
- 新重要记忆：
- 已完成：
- 未完成：
- 风险/阻塞：
- 下一步：

不得只改代码不改本文件。

## 23. 当前重要记忆

- 项目主目标是“看号 + 规划 + 可控自动化”，不是单纯 OCR。
- 主接入通道是 MuMu + ADB。
- 主 GUI 固定为 eframe/egui。
- 主数据库固定为 SQLite。
- PRTS 是养成与干员数据主来源，但必须做校验与降级。
- 官方公告是活动开始时间第一可信源。
- Penguin 是掉率收益主来源。
- DeepSeek 只能辅助，不能单独决定数据库最终态。
- 体力药是时机型资源，不可简单“有药就吃”。
- 无人机属于可重复利用资源，默认可调度。
- 合成玉、源石、寻访资源必须默认硬保护。
- “快速精二/专精”通过 Floor Profile 落地，而不是模糊口号。
- 共享配置当前放在 `akbox-core::config`，默认配置文件名为 `ArkAgent.toml`，缺失时回退默认值而不是报错。
- 共享文件日志当前通过 `akbox-core::logging` 初始化，默认写入工作目录下的 `logs/arkagent.log`，文件级别收敛到 `INFO`。
- desktop 当前已具备可实操的 Dashboard / Settings 壳层，设置页可查看、编辑、保存配置并写入测试日志。
- desktop 后续默认界面语言以中文为准，字体统一收敛到项目内嵌的思源黑体 Regular，避免依赖系统字体回退。
- 当前 desktop 已内嵌 `assets/fonts/SourceHanSansSC-Regular.otf`，GUI 启动时会主动注册并优先使用这套字体。
- 当前仓库已提供 `scripts/build-desktop.ps1`，默认生成 `dist\方舟看号台.exe`，用于避免继续误开英文文件名的历史桌面产物。
- 调试产物导出当前统一经 `akbox-core::debug_artifact` 处理；CLI 可用 `akbox-cli debug export-sample [config_path]`，desktop 设置页可直接导出样例 PNG 与识别 JSON。
- SQLite 当前统一经 `akbox-data::database::AppDatabase` 管理：默认路径为工作目录下 `data\arkagent.db`，连接时启用 WAL 和 foreign keys，并自动应用根目录 `migrations/0001_initial.sql`。
- desktop 面向用户的“截图导出”入口后续必须指向真实设备截图链路，不能继续把占位样例 PNG 包装成“真实截图”；在 M4 之前可以先保留 UI 与接口占位，但文案和行为必须准确。
- `akbox-device` 当前已暴露 `capture_device_screenshot_png` 与 `ScreenshotCaptureRequest` 作为 M4 前置接口；现阶段该接口会明确返回“阶段 4 尚未接入 MuMu / ADB 截图链路”，desktop 已改为调用这条真实入口而不再导出占位样例图。
- PRTS 的早期 `sync prts`“仅站点锚点”语义已被后续全量同步取代；当前 CLI `akbox-cli sync prts [database_path]` 与 desktop“同步 PRTS 全部”都会顺序执行 siteinfo / operator / item / stage / recipe 五段同步，并分别回写对应的 `raw_source_cache` 与 `sync_source_state`。
- M3 后续除了 CLI 入口外，还要把 PRTS 与 Penguin 的同步结果直接暴露到 GUI 标签页；当前轮次展示内容先以“状态 + 缓存摘要 + 若干结果行”为主，先保证能看、能验证，再根据用户反馈收缩。
- desktop 现在已有独立“同步”页，并提供 `PRTS` / `官方公告` / `Penguin` 三个标签；同步动作通过后台线程执行，避免直接阻塞 GUI 事件循环。
- Penguin 当前同步入口固定为 `https://penguin-stats.io/PenguinStats/api/v2/result/matrix?server=CN`；成功时会写入 `raw_source_cache(cache_key = penguin:matrix:cn)`、更新 `sync_source_state(source_id = penguin.matrix.cn)`，并刷新 `external_drop_matrix`。
- Penguin 当前链路已确认存在瞬时网络抖动：同机 `curl` 与独立 `reqwest` 直连可达，但 `sync penguin` 偶发会在 `send request` 阶段失败；现已在 `PenguinClient` 内增加有限次重试与告警日志，优先兜住这类瞬时失败。
- Penguin 的“当前可访问关卡 / 正在进行中的活动”判定不能只看 `result matrix` 的时间窗；还必须结合 `stages.existence.CN.exist/openTime/closeTime`。已用实时 CN `stages` 数据确认，旧活动如 `GT-1` 仍保留 `stageType = ACTIVITY`，但 `closeTime` 早已结束；若只看 matrix 或只看 stage type，会误把历史活动排进“活动优先”组。
- 进一步确认：实时 Penguin CN `stages` 中，`ACTIVITY` 关卡会带明确的 `openTime/closeTime`，而常驻 `MAIN/DAILY` 一般只有 `exist = true`。因此当前预览对活动类关卡应采用更保守策略：拿不到明确活动窗口就不展示，宁可漏掉未同步完成的活动，也不能继续把老活动展示成“当前掉落”。
- 进一步排查发现，Penguin 里还存在大量 `*_perm` 常驻化活动关卡：它们仍然是 `stageType = ACTIVITY`，也有 `openTime`，但没有 `closeTime`。这类关卡属于“当前可访问的其他关卡”，不应被当成“正在进行中的活动”排进优先组；活动优先组必须再收紧为“有明确 `closeTime` 且当前尚未结束的限时活动”。
- 继续排查发现，Penguin 原始 `stages` 里还包含 `stageId = recruit`、`code = 公开招募`、`apCost = 99` 这类非战斗伪关卡，并把公开招募标签当成“掉落”。当前掉落预览必须显式排除这类非刷图关卡，不能仅依赖 `stageType` 或 `existence` 做判断。
- 官方公告当前同步入口固定为 `https://ak.hypergryph.com/news`；实现上直接解析官方页面内嵌的 Next.js flight payload，不依赖额外私有接口；成功时会写入 `raw_source_cache(cache_key = official:notice:cn)`、更新 `sync_source_state(source_id = official.notice.cn)`，并 upsert 到 `external_event_notice`。
- PRTS 的首个结构化业务数据入口当前落在 MediaWiki API `action=parse&page=道具一览&prop=revid|text&format=json`；同步入口为 `akbox-cli sync prts-items [database_path]`，成功时会写入 `raw_source_cache(cache_key = prts:item-index:cn)`、更新 `sync_source_state(source_id = prts.item-index.cn)`，并 upsert 到 `external_item_def`；Penguin 的 item stub 在主键冲突时只保留占位插入，不再覆盖 PRTS 已同步的正式道具定义。
- PRTS 的关卡静态映射当前通过两步 MediaWiki API 组合获取：先用 `action=parse&page=关卡一览&prop=revid&format=json` 取 revision，再用 `action=ask&query=[[关卡id::+]]|?关卡id|?分类|limit=500[|offset=n]&format=json` 分页拉取结构化关卡索引；同步入口为 `akbox-cli sync prts-stages [database_path]`，成功时会写入 `raw_source_cache(cache_key = prts:stage-index:cn)`、更新 `sync_source_state(source_id = prts.stage-index.cn)`，并把 PRTS 负载挂到 `external_stage_def.raw_json.$.prts`，避免覆盖 Penguin 的 stage 根对象。
- PRTS 配方当前通过 MediaWiki API `action=parse&page=罗德岛基建/加工站&prop=revid|text&format=json` 获取，再解析加工站配方表落到 `external_recipe`；同步入口为 `akbox-cli sync prts-recipes [database_path]`，成功时会写入 `raw_source_cache(cache_key = prts:recipe-index:cn)`、更新 `sync_source_state(source_id = prts.recipe-index.cn)`；由于实时页面里存在相同产物/等级的重复配方行，当前 `recipe_id` 采用 `workshop:{output_item_id}:lv{level}:row{n}`，并在每次同步时全量替换 `external_recipe`。
- PRTS 配方里的道具名解析当前不能简单按“同名即报错”处理；与 Penguin 共库时会出现同名旧 id / 别名 item（如 `碳` 同时命中 `200008` 与 `3112`）。当前规则是：同名时优先选择带 PRTS 正式负载的定义；若都只来自 Penguin，再优先选择 `item_id == sortId` 的 canonical 项；只有仍然无法判定时才报真正的歧义错误。
- PRTS 干员定义里，`分类:专属干员` 当前可作为“模式 / 活动专属、玩家常规 box 不可拥有干员”的稳定标记；`sync prts-operators` / `sync prts` 现会在写入 `external_operator_def` 前过滤这类干员，并采用替换写入而不是纯 upsert，确保旧库里残留的 `Mechanist(卫戍协议)`、`暮落(集成战略)`、预备干员等条目会在下次同步时被清掉。
- desktop 里的 PRTS 同步入口后续不再长期维持“站点 / 道具 / 关卡”多个分散按钮；随着 PRTS 结构化同步项增加，应收敛成一个“同步 PRTS 全部”按钮，在后台顺序执行当前所有 PRTS 子同步并统一回填概览。
- desktop 同步页后续必须按“玩家可读”而不是“源数据直出”展示：不再默认展示 `source_id` / `cache_key` / `content_type`；时间统一转换到用户配置的时区；Penguin 需要把 `main_01-07` 之类的 stage id 转成玩家可读名称、把 item id 转成游戏内道具名、按关卡聚合并按掉率降序展示材料，同时展示单材料期望体力；当前掉落预览还需要按关卡热度（最近一段时间的上传数量）排序，并优先展示“正在进行中的活动里且掉落蓝色材料”的关卡，其余部分再展示“当前可访问的全部关卡”；掉落展示上要区分常规掉落与特殊掉落，`EXTRA_DROP` / 额外物资默认折叠且不展示；官方公告后续只应展示真正的活动公告，创作征集、制作组通讯等内容先记录需求，后续可结合规则或 DeepSeek 辅助过滤。
- desktop 的长页面需要默认具备滚动能力，不能因为内容变长导致底部信息被截断；本轮同步页收口时一并补上页面滚动容器。

## 24. 当前待办（初始）

1. 初始化 Workspace（已完成）
2. 创建 desktop / cli / core / data / device / testkit 五个包（已完成）
3. 启动空 GUI（已完成）
4. 提供 CLI 空命令与 `--help`（已完成）
5. 建立配置读取（已完成）
6. 建立日志（已完成）
7. 在不跳阶段的前提下，把配置与日志接入 GUI，先做出可查看/编辑/保存配置的实操页面（已完成）
8. 将现有 GUI 全面切换为中文，并为 desktop 内嵌思源黑体 Regular（已完成）
9. 支持 debug 模式下的调试产物导出能力，作为 M1 收尾（已完成）
10. 建立 SQLite migration（已完成）
11. 打通外部数据同步骨架（进行中：PRTS 站点 / PRTS 干员基础资料 / PRTS 道具索引 / PRTS 关卡静态映射 / PRTS 配方 / Penguin / 官方公告 已完成且已接入 GUI；干员定义现已按 `分类:专属干员` 过滤 box 不可拥有的临时干员；同步页首轮玩家可读收口、长页面滚动、Penguin 预览排序 / 掉落分组 / 当前可访问判定已完成；desktop 与 CLI 的 PRTS 入口已收敛为全量同步；下一步继续停留在 PRTS 主线，转入干员养成需求或基建技能）
12. 建立 AGENTS.md 更新习惯（进行中，已完成多次记录）
13. 将 desktop 的“导出调试样例”改为真实截图导出入口，为 M4 的 ADB 截图接入预留 UI 和接口，但不提前实现真实抓图（已完成）
14. 为 PRTS 与 Penguin 增加 GUI 标签页展示当前同步内容与结果摘要（已完成）
15. 为官方公告增加 GUI 标签页展示同步状态与公告摘要（已完成）

## 25. 已完成内容（初始）

- 需求已整理进 AGENTS.md 初稿
- 总体架构、模块边界、开发路线、资源策略已冻结为首版执行规范
- 已创建 Cargo Workspace 根清单、`rust-toolchain.toml`、`.gitignore`
- 已创建 `apps/`、`crates/`、`assets/`、`docs/`、`migrations/` 基础目录结构
- 已生成 `akbox-desktop`、`akbox-cli`、`akbox-core`、`akbox-data`、`akbox-device`、`akbox-testkit` 六个基础包
- 已将 `akbox-desktop` 切换为 `eframe/egui` 空壳主窗口并完成静态验证
- 已通过前台运行超时验证确认桌面程序能进入事件循环，满足 M0 对 GUI 空窗体的最低要求
- 已将 `akbox-cli` 切换为项目级帮助入口，并验证 `cargo run -p akbox-cli -- --help` 输出
- M0 / 阶段 0：工程初始化已完成，下一阶段转入配置与日志
- 已建立共享配置模型、默认值与 TOML 读取能力，支持 ADB 可执行路径和游戏时区配置
- 已为 CLI 增加 `debug config [path]` 调试入口，可打印解析后的配置来源、ADB 路径与游戏时区
- 已建立共享文件日志初始化能力，CLI 与 desktop 共用 `logs/arkagent.log` 文件输出
- 已将 desktop 从空窗体扩展为带 Dashboard / Settings 的实操壳，支持配置查看、编辑、保存、重载与测试日志写入
- 已将 desktop 全面切换为中文界面，并内嵌思源黑体 Regular 作为默认 GUI 字体
- 已为 `akbox-data` 增加同步失败告警写入/恢复、同步状态查询、原始缓存摘要查询，以及 `external_drop_matrix` 的基础读写接口
- 已打通 Penguin CN 掉率矩阵同步链路，支持 `akbox-cli sync penguin [database_path]`，并把矩阵原始响应缓存到 SQLite
- 已为 desktop 增加“同步”页和 `PRTS` / `Penguin` 标签，可查看同步状态、缓存摘要与 Penguin 掉率矩阵样例，并在后台线程中触发真实同步
- 已在 `assets/fonts/` 中补充字体来源说明，记录 Adobe 官方仓库与 release 资产路径
- 已提供中文桌面构建脚本，可产出 `dist\方舟看号台.exe` 作为固定双击入口，并完成启动验证
- 已完成 M1 调试产物导出闭环：共享导出模块、CLI 调试命令、desktop 设置页导出按钮均已就位，并验证能实际写出 PNG/JSON
- 已完成 M2 / 阶段 2：SQLite 连接、migration 系统、核心表骨架、repository 基础接口与 `audit_log` 写入能力
- 已将 desktop 的样例截图导出按钮替换为真实截图导出入口，并通过 `akbox-device` 的设备截图接口占位为 M4 预留接入点
- 已完成 M3 的第一个最小闭环：PRTS 客户端、原始响应缓存、同步状态写库与 CLI `sync prts` 入口
- 已完成 M3 的第二个最小闭环：官方公告客户端、`external_event_notice` 写库、CLI `sync official [database_path]`、desktop “官方公告”同步标签与公告样例展示
- 已完成 M3 的第三个最小闭环：PRTS 道具索引客户端、`external_item_def` 写库、CLI `sync prts-items [database_path]`，以及 desktop `PRTS` 标签中的道具索引状态与样例展示
- 已完成 M3 的第四个最小闭环：PRTS 关卡静态映射客户端、`external_stage_def` 写库、CLI `sync prts-stages [database_path]`，以及 desktop `PRTS` 标签中的关卡索引状态与样例展示；`external_stage_def.raw_json` 现已保留 Penguin 根对象并把 PRTS 负载挂到 `$.prts`
- 已完成 M3 的第五个最小闭环：PRTS 配方客户端、`external_recipe` 写库、CLI `sync prts-recipes [database_path]`，以及 desktop `PRTS` 标签中的配方状态与样例展示
- 已完成 M3 的第六个最小闭环：PRTS 干员基础资料客户端、`external_operator_def` 写库、CLI `sync prts-operators [database_path]`，以及 desktop `PRTS` 标签中的干员索引状态与样例展示；当前会按 PRTS `分类:专属干员` 过滤掉卫戍协议 / 集成战略 / 预备干员等 box 不可拥有条目，并在同步时清理旧残留定义
- 已将 PRTS 的用户入口收敛为全量同步：CLI `sync prts [database_path]` 现在顺序执行 siteinfo / operator / item / stage / recipe，desktop 同步页也改为单个“同步 PRTS 全部”按钮
- 已修复 PRTS 配方同步与 Penguin 共库时的同名道具歧义：当前会优先选 PRTS 正式定义，再处理 Penguin canonical sortId，避免 `碳 -> 200008 / 3112` 这类别名冲突把全量同步卡死
- 已为 Penguin 同步补上有限次重试与单元测试，优先兜住偶发的 `send request` / 临时 5xx 失败，避免把瞬时网络抖动直接固化为同步失败告警
- 已为 desktop 长页面补上统一滚动容器，并将同步页首轮收口为玩家可读展示：隐藏 `source_id` / `cache_key` / `content_type`，时间按用户配置时区展示；Penguin 预览改为按关卡聚合展示当前掉落、道具名与期望体力
- 已将 Penguin 当前掉落预览细化为“活动蓝材优先 + 最近上传量降序”的关卡排序，并区分常规掉落 / 特殊掉落；`EXTRA_DROP` / 家具等额外物资默认折叠不展示
- 已修正 Penguin 当前掉落预览的活动判定：现在只展示当前可访问的关卡；活动优先组必须同时满足“活动关卡 + CN 开放窗口当前有效 + 掉落蓝色材料”，避免把已结束活动误排进前列

## 29. 变更记录

### 变更记录模板

- 日期时间：2026-03-15 23:22:18 +08:00
- 阶段：M0 / 阶段 0：工程初始化
- 新需求：按仓库根目录 `AGENTS.md` 执行，并严格从 M0 开始，不跳步；每完成一个有意义的步骤都更新 `AGENTS.md`
- 新重要记忆：M0 首个最小闭环已确定为“workspace 与工具链 + 基础目录结构”，后续仍按 `desktop 空窗体 -> cli 空命令` 顺序推进
- 已完成：创建 Cargo Workspace；固定 `nightly-2025-07-12` 工具链；建立基础目录结构与六个基础包；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets`、`cargo test --workspace` 全部通过
- 未完成：`akbox-desktop` 仍是 Cargo 默认模板，尚未切换到 `eframe/egui` 空壳窗口；`akbox-cli` 仍是 Cargo 默认模板，尚未整理成项目入口
- 风险/阻塞：当前无阻塞；后续引入 GUI 依赖时需要首次下载 crates
- 下一步：实现 `apps/akbox-desktop` 的 eframe/egui 空壳主窗口，并再次执行 fmt/clippy/test 后回写本文件

### 变更记录

- 日期时间：2026-03-15 23:24:49 +08:00
- 阶段：M0 / 阶段 0：工程初始化
- 新需求：无
- 新重要记忆：M0 的 GUI 验证采用“两段式”执行：先用 `cargo clippy` 与 `cargo test` 完成构建验证，再用前台运行超时确认窗口已进入事件循环
- 已完成：为 workspace 增加 `eframe` 共享依赖；将 `apps/akbox-desktop` 改为 `eframe/egui` 空壳窗口；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；前台运行 `cargo run -p akbox-desktop` 在超时前未退出，可推定窗口已成功启动
- 未完成：`akbox-cli` 仍是 Cargo 默认模板，尚未提供项目级 `--help` 输出；M0 尚未整体收尾
- 风险/阻塞：GUI 运行验证依赖超时推断而非人工截图；当前环境下未保留窗口截图证据
- 下一步：实现 `apps/akbox-cli` 的空命令入口，保证 `cargo run -p akbox-cli -- --help` 输出项目帮助，然后再次执行 fmt/clippy/test 并更新本文件

### 变更记录

- 日期时间：2026-03-15 23:26:41 +08:00
- 阶段：M0 / 阶段 0：工程初始化
- 新需求：无
- 新重要记忆：M0 的 CLI 先保持零外部依赖，使用手写参数分派稳定 `--help` 与预留子命令接口，避免在阶段 0 过早扩大依赖面
- 已完成：实现 `apps/akbox-cli` 的帮助入口与预留子命令；新增 CLI 单元测试；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；执行 `cargo run -p akbox-cli -- --help` 返回预期帮助文本；M0 整体完成
- 未完成：M1 / 阶段 1 的配置读取、ADB 路径配置、游戏时区配置、日志文件输出与 debug 导出能力尚未开始
- 风险/阻塞：当前四个 library crate 仍保留 Cargo 默认模板测试，后续进入对应阶段前需要替换为项目真实骨架
- 下一步：按既定顺序进入 M1 / 阶段 1，先建立共享配置模型与配置文件读取，并将 ADB 路径和游戏时区纳入配置结构

### 变更记录

- 日期时间：2026-03-15 23:46:34 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：无
- 新重要记忆：共享配置当前统一放在 `akbox-core::config`；默认从工作目录下的 `ArkAgent.toml` 读取，若文件不存在则使用内置默认配置；CLI 已提供 `debug config [path]` 作为配置调试入口
- 已完成：在 `akbox-core` 中新增 `AppConfig`、`AdbConfig`、`GameConfig`、`ConfigSource` 与 TOML 读取逻辑；支持默认 `Asia/Shanghai` 游戏时区、可选 ADB 可执行文件路径、默认路径回退；在 `akbox-cli` 中接入 `debug config [path]` 命令；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；执行 `cargo run -p akbox-cli -- debug config <temp-config>` 成功读取临时配置并输出 `ADB executable: C:/MuMu/adb.exe` 与 `Game timezone: UTC`
- 未完成：日志文件输出、debug 模式下截图与识别结果导出尚未开始；desktop 端尚未消费共享配置
- 风险/阻塞：当前配置解析仅校验非空值，尚未对时区标识做更严格校验；默认配置搜索范围目前仅覆盖工作目录下的 `ArkAgent.toml`
- 下一步：继续 M1 / 阶段 1，建立日志初始化能力，先实现文件日志落地与 desktop/cli 共用的启动日志入口

### 变更记录

- 日期时间：2026-03-15 23:59:03 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：继续推进，直到 GUI 出现可以实操的内容；仍需严格按里程碑顺序执行，不得跳到后续阶段
- 新重要记忆：为尽快达成“GUI 可实操”，当前阶段采用“先补日志，再把配置与日志接入 desktop 设置页”的顺序推进；运行时本地产物加入 `.gitignore`
- 已完成：扩展 `AppConfig`，新增日志与调试导出配置项，并支持 `save` / `save_to_path`；新增 `akbox-core::logging` 共享日志初始化模块；CLI 启动时按配置初始化文件日志；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；执行 `cargo run -p akbox-cli -- debug config` 成功输出配置路径、日志路径与调试导出目录；`logs/arkagent.log` 已落盘并写入 `logging initialized` 与 `cli command started`
- 未完成：desktop 尚未消费这些能力形成可操作界面；M1 的调试产物导出能力尚未补齐
- 风险/阻塞：日志文件里保留了早期一次 `TRACE` 级别验证遗留记录，后续新写入已收敛到 `INFO`；调试导出当前仍停留在配置层
- 下一步：把配置与日志接入 desktop，先做出可查看、编辑、保存配置并写测试日志的设置页，达到“GUI 可实操”

### 变更记录

- 日期时间：2026-03-15 23:59:03 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：无
- 新重要记忆：desktop 的第一个可实操页面固定为 Settings，不引入文件选择器等额外依赖，先用文本输入 + 保存/重载按钮完成本地配置闭环
- 已完成：`apps/akbox-desktop` 已接入共享配置与日志；新增 Dashboard / Settings 导航；Settings 页支持查看配置来源与保存路径、编辑 ADB 路径/游戏时区/日志目录/日志文件名/调试导出目录、切换调试导出开关、保存到 `ArkAgent.toml`、从磁盘重载、写入测试日志；新增 desktop 纯逻辑单元测试；再次执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；执行 `cargo run -p akbox-desktop` 在超时前持续运行，且 `logs/arkagent.log` 记录了 `desktop app starting`
- 未完成：M1 仍缺“debug 模式下导出截图与识别结果”的实际产物导出能力；数据库与 migration 尚未开始
- 风险/阻塞：当前 GUI 实操能力已具备，但本轮只验证了启动与日志落盘，尚未通过自动化手段点击 UI 控件；日志路径变更保存后需重启应用才会切换到新文件
- 下一步：继续收尾 M1，实现调试产物导出目录与基础导出接口，然后再进入 SQLite migration

### 变更记录

- 日期时间：2026-03-16 00:05:10 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：GUI 全面使用中文；为项目内嵌思源黑体 Regular，避免依赖系统字体回退
- 新重要记忆：desktop 现已在启动阶段注册内嵌 `SourceHanSansSC-Regular.otf`，并将其插入 `egui` 的 `Proportional` 与 `Monospace` 字体族首位；GUI 文案本地化在 desktop 层处理，不影响 CLI
- 已完成：从 Adobe 官方 `source-han-sans` 2.005R release 的 `09_SourceHanSansSC.zip` 提取 `OTF/SimplifiedChinese/SourceHanSansSC-Regular.otf`，放入 `assets/fonts/`；新增 `assets/fonts/README.md` 记录来源与许可证链接；将 desktop 可见文案、按钮、状态提示、配置来源描述切换为中文；在 `apps/akbox-desktop` 中嵌入并启用思源黑体 Regular；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；直接启动 `target\\debug\\akbox-desktop.exe` 5 秒后进程仍在运行，可推定内嵌字体已正常加载
- 未完成：M1 仍缺“debug 模式下导出截图与识别结果”的实际产物导出能力；SQLite 与 migration 尚未开始
- 风险/阻塞：当前内嵌的是简体中文专用 `SourceHanSansSC-Regular.otf`，文件体积约 15.8 MiB；若后续加入大量代码/等宽文本展示，可能需要补专门的 monospace 策略
- 下一步：继续完成 M1 收尾，实现调试产物导出目录与基础导出接口，然后进入 SQLite migration

### 变更记录

- 日期时间：2026-03-16 00:05:10 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：用户反馈直接打开 exe 仍能看到英文，需要继续清理 GUI 残余英文并确保实际可打开的 exe 被重新构建
- 新重要记忆：仅通过 `clippy` / `test` 不能保证 `target\\debug\\akbox-desktop.exe` 时间戳更新；用户直接打开 exe 时需要显式执行 `cargo build -p akbox-desktop`
- 已完成：已定位到 desktop 中仍残留窗口标题、顶部标题等少量英文/英文品牌；已确认当前 `target\\debug\\akbox-desktop.exe` 的时间戳落后于最近一次中文化改动
- 未完成：desktop 仍需彻底去掉残余英文可见文案，并重新构建新的 exe 供直接启动验证
- 风险/阻塞：如果用户继续打开旧的构建产物，即使源码已修改也不会看到中文界面
- 下一步：清理 desktop 残余英文可见文案，显式执行 `cargo build -p akbox-desktop`，再验证新的 exe 启动

### 变更记录

- 日期时间：2026-03-16 00:08:33 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：无
- 新重要记忆：用户直接打开 exe 时，除了源码文案外，还必须确认 `target\\debug\\akbox-desktop.exe` 已被显式重建；当前可直接验证的中文 exe 为该路径下 2026-03-16 00:08:07 生成的二进制
- 已完成：清理 desktop 中剩余用户可见英文，包括窗口标题 `ArkAgent 看号台`、顶部标题 `ArkAgent`、按钮 `保存到 ArkAgent.toml`、表单标签 `ADB 可执行文件`；改为中文后再次执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；显式执行 `cargo build -p akbox-desktop`，成功生成新的 `target\\debug\\akbox-desktop.exe`；直接启动重建后的 exe 5 秒后进程仍在运行
- 未完成：M1 仍缺调试产物导出能力；SQLite 与 migration 尚未开始
- 风险/阻塞：如果用户打开的是仓库外部复制出去的旧 exe，界面仍可能是旧版本；当前只重建了 `debug` 目标，没有额外产出 `release` 包
- 下一步：优先让用户使用 `C:\\Users\\emmmer.SUPERXLB\\git\\ArkAgent\\target\\debug\\akbox-desktop.exe` 验证中文界面；随后继续收尾 M1 的调试产物导出能力

### 变更记录

- 日期时间：2026-03-16 00:11:33 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：用户反馈“打开 exe 还是显示英文”，除了 GUI 文案中文化外，还需要给出固定的中文桌面产物路径，降低误开旧英文文件名二进制的概率
- 新重要记忆：桌面端验证不能只停留在 `cargo run` 或 `target\\debug\\akbox-desktop.exe`；需要补一个稳定的中文产物出口，便于直接双击验收
- 已完成：已确认当前源码中的窗口标题与 GUI 可见文案均为中文，运行中的主窗口标题为“方舟看号台”
- 未完成：仓库内还没有固定的中文桌面产物输出路径，用户仍可能继续打开英文文件名的历史可执行文件
- 风险/阻塞：若仅依赖默认 Cargo 产物名，Windows 侧仍会暴露 `akbox-desktop.exe` 这个英文文件名，用户容易与旧构建混淆
- 下一步：增加桌面构建脚本，输出 `dist\\方舟看号台.exe` 作为固定中文双击入口，并完成构建验证

### 变更记录

- 日期时间：2026-03-16 00:13:30 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：无
- 新重要记忆：桌面端现在应优先通过 `dist\\方舟看号台.exe` 做人工验收；中文文件名会直接反映到 Windows 进程名，能进一步减少“exe 还是英文”的感知问题
- 已完成：新增 `scripts\\build-desktop.ps1`，默认执行 release 构建并复制产物到 `dist\\方舟看号台.exe`；`.gitignore` 已忽略 `dist/`；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；执行脚本后成功生成中文 exe，并验证其 `ProcessName` 与 `MainWindowTitle` 均为“方舟看号台”
- 未完成：M1 仍缺调试产物导出能力；SQLite 与 migration 尚未开始
- 风险/阻塞：当前中文双击产物来自构建后复制，源码默认 Cargo 原生产物名仍是 `akbox-desktop.exe`；若用户继续手动打开旧路径，仍可能绕过新的中文入口
- 下一步：让用户优先验证 `C:\\Users\\emmmer.SUPERXLB\\git\\ArkAgent\\dist\\方舟看号台.exe`；随后继续完成 M1 的调试产物导出能力

### 变更记录

- 日期时间：2026-03-16 00:22:14 +08:00
- 阶段：M1 / 阶段 1：配置与日志
- 新需求：无
- 新重要记忆：调试产物导出当前通过 `akbox-core::debug_artifact` 提供共享实现，导出开关关闭时会显式返回“已跳过”而不是静默失败；CLI 与 desktop 复用同一套导出逻辑
- 已完成：新增共享调试导出模块，支持按配置导出截图 PNG 与识别结果 JSON；为 CLI 增加 `debug export-sample [config_path]`；为 desktop 设置页与仪表盘增加“导出调试样例”入口；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；执行 `cargo run -p akbox-cli -- debug export-sample <temp-config>` 实际写出 `1773591726160-screenshot-cli-debug.png` 与 `1773591726160-recognition-cli-debug.json`；M1 / 阶段 1 现已完成
- 未完成：SQLite 与 migration 尚未开始
- 风险/阻塞：desktop 侧本轮只完成了编译与共享逻辑接入，尚未自动化点击 GUI 按钮做端到端 UI 交互验证；当前导出的是样例截图与样例识别结果，后续接入设备与视觉层后需要替换为真实采集产物
- 下一步：进入 M2 / 阶段 2，先建立 SQLite 连接、migration 系统与核心表骨架，再补 repository 基础接口

### 变更记录

- 日期时间：2026-03-16 00:26:30 +08:00
- 阶段：M2 / 阶段 2：SQLite 与 migration
- 新需求：无
- 新重要记忆：SQLite 首版 schema 统一收敛在根目录 `migrations/0001_initial.sql`；`AppDatabase::open` 会创建父目录、打开数据库、启用 WAL/foreign keys，并自动应用 migration；`AppRepository` 当前先提供 `app_meta` upsert/get 与 `audit_log` 追加写入作为 repository 基础接口
- 已完成：为 workspace 增加 `rusqlite` 与 `rusqlite_migration`；重写 `akbox-data`，新增 `database.rs`、`repository.rs` 和导出入口；建立 `app_meta`、`sync_source_state`、`raw_source_cache`、全部外部定义表、库存/干员快照与当前态表、`scan_artifact`、`recognition_review_queue`、`resource_policy`、`floor_profile`、`planner_run`、`base_layout_config`、`alert`、`audit_log` 等核心表；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；相关测试已验证所有核心表存在、WAL 已启用、`app_meta` upsert 可用、`audit_log` 可落库；M2 / 阶段 2 现已完成
- 未完成：M3 / 阶段 3 的 PRTS、官方公告、Penguin 同步客户端与缓存编排尚未开始
- 风险/阻塞：当前 schema 仍是 v1 骨架，很多业务列先以 `raw_json` / `payload_json` 兜底，后续进入真实同步与扫描阶段时需要逐步细化字段；数据库路径尚未进入配置文件，当前默认采用工作目录下 `data\arkagent.db`
- 下一步：进入 M3 / 阶段 3，先搭建外部数据同步骨架，从 PRTS 客户端与原始响应缓存开始

### 变更记录

- 日期时间：2026-03-16 00:36:25 +08:00
- 阶段：M2 / 阶段 2：SQLite 与 migration（补充 UI/接口校正）
- 新需求：将 desktop 中误导性的“导出调试样例”改为真实截图导出入口，为后续 M4 的 ADB 截图接入做前置 UI
- 新重要记忆：desktop 不再把样例 PNG 暴露为“截图”；面向用户的截图按钮现在统一走 `akbox-device::capture_device_screenshot_png` 这条真实设备截图入口，即使当前尚未实现，也必须返回准确状态
- 已完成：新增 `akbox-device` 的 `ScreenshotCaptureRequest` 与 `capture_device_screenshot_png` 占位接口，并为其补充单元测试；desktop 现已依赖 `akbox-device`，将仪表盘/设置页中的“导出调试样例”改为“导出真实截图”，并在界面上明确标注截图来源是 “MuMu / ADB 设备截图（阶段 4 接入后可用）”；按钮点击后不再导出占位图，而是明确提示“阶段 4 尚未接入 MuMu / ADB 截图链路”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；直接启动 `target\debug\akbox-desktop.exe` 3 秒后进程仍在运行
- 未完成：真实的 MuMu 设备发现、ADB 连接与截图抓取仍未开始；当前 CLI 的 `debug export-sample` 仍保留样例文件导出能力，主要用于验证调试产物落盘链路
- 风险/阻塞：当前按钮已是“真实截图入口”，但在 M4 之前只能返回未接入提示，不能给用户产生实际截图文件；若后续要把 CLI 也切到真实截图链路，需要等设备层准备好后再统一调整
- 下一步：继续按里程碑进入 M3 / 阶段 3，先搭建 PRTS 同步客户端与原始响应缓存

### 变更记录

- 日期时间：2026-03-16 00:42:57 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：无
- 新重要记忆：M3 的第一个落地点选定为 PRTS 的 MediaWiki API `siteinfo` 查询；当前同步入口通过 `akbox-cli sync prts [database_path]` 驱动，默认把数据库放到工作目录 `data\arkagent.db`；成功时会写入 `raw_source_cache(cache_key = prts:siteinfo:general)` 并更新 `sync_source_state(source_id = prts.siteinfo.general)`
- 已完成：为 workspace 增加 `reqwest + rustls`；在 `akbox-data` 中新增 `prts.rs`、`sync.rs`，实现 `PrtsClient`、`sync_prts_site_info` 和 `raw_source_cache / sync_source_state` 的 repository 写入；为 `akbox-cli` 增加 `sync prts [database_path]` 与 `sync --help`；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -p akbox-cli -- sync prts <temp-db>`，成功拿到 PRTS 返回并写入本地 SQLite，命令输出中的 `Revision` 为 `2026-03-15T16:42:39Z`，缓存字节数为 `1820`
- 未完成：官方公告客户端、Penguin 客户端、同步失败告警写入、外部定义表的结构化落库尚未开始
- 风险/阻塞：当前 PRTS 只打通了 `siteinfo` 这一条最小链路，用于验证 API 连通性与缓存机制；还没有开始同步干员、道具、配方等真正业务数据；CLI 真实同步目前依赖公网访问
- 下一步：继续 M3，优先接官方公告客户端，并把同步失败写入 `alert`，为后续企鹅数据接入复用同一套骨架

### 变更记录

- 日期时间：2026-03-16 01:00:40 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：为 PRTS 增加 GUI 标签页展示同步内容；继续接入 Penguin，并增加 GUI 标签页展示其同步内容；当前轮次展示字段先不做过度收缩，后续再根据用户反馈 shrink
- 新重要记忆：desktop 已新增“同步”页，内含 `PRTS` / `Penguin` 标签；当前通过后台线程执行同步任务，再回填本地概览，避免把网络请求直接堵在 GUI 线程上；Penguin 当前最小落地点是 CN `result matrix`，并以 `external_drop_matrix` + `raw_source_cache` + `sync_source_state` 作为 GUI 展示来源
- 已完成：在 `akbox-data` 中新增 `penguin.rs`，打通 `https://penguin-stats.io/PenguinStats/api/v2/result/matrix?server=CN`；为同步骨架补齐同步失败写入 `alert`、成功后 `resolve_alert`、同步状态摘要查询、缓存摘要查询、`external_drop_matrix` 替换写入与样例读取；为 `akbox-cli` 增加 `sync penguin [database_path]` 与帮助文本；为 `apps/akbox-desktop` 增加“同步”页和 `PRTS` / `Penguin` 标签，展示来源状态、缓存版本/字节数/时间、最近错误，以及 Penguin 掉率矩阵样例；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -p akbox-cli -- sync penguin C:\Users\emmmer.SUPERXLB\AppData\Local\Temp\arkagent-penguin-validation-20260316\penguin.db`，成功写入 `7791` 条矩阵记录，`Revision` 为 `1773115200000`，缓存字节数为 `972860`；已实际启动 `target\debug\akbox-desktop.exe` 3 秒并确认窗口标题为“方舟看号台”
- 未完成：官方公告客户端尚未开始；PRTS 仍只同步 `siteinfo`，还没有下钻到干员、道具、配方等业务定义；GUI 同步页当前只展示基础摘要，后续还要按用户反馈收缩字段
- 风险/阻塞：当前 PRTS 与 Penguin 同步都依赖公网；Penguin 的 `external_stage_def` / `external_item_def` 仍是最小 stub 写入，只足够支撑矩阵落库与页面展示，还不能视为完整静态资料同步；GUI 的后台同步虽已避免主线程长阻塞，但尚未做更细粒度进度回传
- 下一步：继续 M3，优先接官方公告客户端，并把同步结果接到现有“同步”页；随后再回头扩展 PRTS 的结构化业务数据同步

### 变更记录

- 日期时间：2026-03-16 15:16:58 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：继续严格按里程碑推进；在 PRTS 与 Penguin 之后补齐官方公告客户端，并在每个有意义步骤后回写 `AGENTS.md`
- 新重要记忆：官方公告当前直接读取 `https://ak.hypergryph.com/news` 的官方页面，并解析其中内嵌的 Next.js flight payload；当前同步入口为 `akbox-cli sync official [database_path]`，成功时会写入 `raw_source_cache(cache_key = official:notice:cn)`、更新 `sync_source_state(source_id = official.notice.cn)`，并 upsert 到 `external_event_notice`
- 已完成：为 workspace 增加 `time` 依赖；在 `akbox-data` 中新增 `official.rs`，实现官方公告 HTML 抓取、flight payload 解码、`NOTICE/ACTIVITY/NEWS` 列表解析，以及活动窗口/维护窗口的时间提取；为 repository 增加 `external_event_notice` 的 upsert / count / list 接口；为同步骨架增加官方公告同步、失败告警恢复、原始 HTML 缓存与成功状态写库；为 CLI 增加 `sync official [database_path]`；为 desktop 同步页增加“官方公告”按钮、标签与公告摘要表格；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -p akbox-cli -- sync official C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-official-validation-20260316\official.db`，成功写入 `24` 条公告记录，`Revision` 为 `2026-03-14T11:30:00+08:00`，缓存字节数为 `211153`
- 未完成：PRTS 仍只同步 `siteinfo`，还没有下钻到干员、道具、配方等业务定义；GUI 同步页当前仍以基础摘要为主，后续还要根据用户反馈收缩字段并补更细粒度的进度回传
- 风险/阻塞：官方公告当前解析依赖官网页面内嵌的 Next.js flight payload 结构；若官网前端结构发生明显调整，需要同步更新提取逻辑；当前公告时间提取采用规则解析，已覆盖活动时间与维护窗口的常见写法，但还未覆盖所有可能文案变体
- 下一步：继续停留在 M3，回到 PRTS 的结构化业务数据同步，优先把干员/道具/配方等定义从“原始缓存”推进到可写入外部定义表的最小闭环

### 变更记录

- 日期时间：2026-03-16 15:29:23 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：无
- 新重要记忆：PRTS 的首个结构化业务数据闭环已固定为“道具索引 -> `external_item_def`”；当前通过 MediaWiki API `action=parse&page=道具一览&prop=revid|text&format=json` 获取 HTML 片段中的 `smwdata` 属性集合；为避免数据源互相覆盖，Penguin 在 `external_item_def` 上仅做缺失主键的占位插入，不再覆盖 PRTS 已同步的正式道具定义
- 已完成：扩展 `akbox-data::prts`，新增 `fetch_item_index`、`PrtsItemDefinition` 与 HTML 属性解析；为 repository 增加 `external_item_def` 的 upsert / count / list 接口；为同步骨架新增 `sync_prts_item_index`、`PRTS_ITEM_INDEX_SOURCE_ID = prts.item-index.cn`、`PRTS_ITEM_INDEX_CACHE_KEY = prts:item-index:cn`，并补齐失败告警写入与恢复；为 CLI 增加 `sync prts-items [database_path]`；为 desktop 的 `PRTS` 标签增加“同步 PRTS 道具”按钮，并同时展示站点信息与道具索引状态、数量和样例；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -p akbox-cli -- sync prts-items C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-prts-items-eafdab70-660d-44e7-b28b-42d104f3a99c\test.db`，成功写入 `1227` 条道具定义，`Revision` 为 `335500`，缓存字节数为 `1211519`
- 未完成：PRTS 仍未同步干员基础资料、养成需求、基建技能、配方与关卡静态映射；GUI 同步页当前仍以基础状态 + 摘要表格为主，尚未提供更细粒度进度回传
- 风险/阻塞：当前道具索引解析依赖 `道具一览` 页面经 `action=parse` 产出的 `smwdata` 标记与属性命名；若 Wiki 模板重构或属性名调整，需要同步更新解析逻辑；当前 `item_type` 由分类字段归纳得出，后续若要更精细分类可能仍需补充映射规则
- 下一步：继续停留在 M3，从 PRTS 里再选一个结构化业务数据最小闭环，优先在“配方”或“关卡/材料静态映射”之间选其一并落到外部定义表

### 变更记录

- 日期时间：2026-03-16 15:37:51 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：检查并缓解 Penguin 同步的“最近错误：failed to send request to Penguin Stats”问题
- 新重要记忆：已确认 Penguin CN 接口不是持续不可达；同机 `curl` 返回 `200 OK`，独立 `reqwest` 与独立 `akbox-data::PenguinClient` 也可成功拉取；故障更接近瞬时网络抖动或 Cloudflare 链路抖动，而不是固定 URL / DNS / 证书配置错误
- 已完成：复现过一次 `cargo run -p akbox-cli -- sync penguin <temp-db>` 的 `send request` 失败；补充对 `https://penguin-stats.io/PenguinStats/api/v2/result/matrix?server=CN` 的直连诊断，确认 `curl` 与 DNS 正常；在 `crates/akbox-data/src/penguin.rs` 中为 Penguin 拉取增加最多 3 次的有限重试，并在重试时写 `tracing::warn!`；新增单测覆盖“首次 503，重试后成功”的场景；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；实网再次执行 `cargo run -q -p akbox-cli -- sync penguin <temp-db>` 成功，当前写入 `7791` 条矩阵记录，`Revision` 为 `1773115200000`，缓存字节数为 `972888`
- 未完成：当前只对 Penguin 先补了有限次重试；PRTS / 官方公告 仍未统一抽象重试策略；GUI 侧的“最近错误”仍依赖下一次成功同步来清空
- 风险/阻塞：当前还没有抓到一次可稳定复现的底层错误链，因此重试属于针对瞬时失败的工程兜底，而不是针对某个已定位根因的定点修复；若后续出现持续性失败，还需要把更详细的 `reqwest` source chain 落到日志或 UI
- 下一步：继续停留在 M3，回到 PRTS 的下一个结构化业务数据最小闭环；若 Penguin 再次出现持续性失败，再补更细的错误链采集与统一重试抽象

### 变更记录

- 日期时间：2026-03-16 15:57:56 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：简化给用户展示的同步信息；不再展示 `source_id` / `cache_key` / `content_type`；时间统一按用户设置时区显示；Penguin 需要用玩家可读的关卡名和道具名展示当前掉落并给出期望体力；desktop 长页面需要滚动条
- 新重要记忆：desktop 的长页面现在统一包在滚动容器中，避免同步页等内容增长后底部信息被截断；Penguin 同步为了支撑玩家可读展示，除矩阵外还会同步 stage/item 元数据，并在本地预览层按“当前有效掉落 -> 关卡聚合 -> 掉率降序”整理展示；官方公告“仅保留活动公告”的过滤需求目前只记录，不在本轮直接删除数据
- 已完成：在 desktop 主内容区补上垂直滚动容器；同步页概览隐藏 `source_id` / `cache_key` / `content_type`，保留状态、时间、缓存体量与最近错误；时间展示改为基于用户配置时区格式化；Penguin 同步增加 stage/item 元数据拉取与写库，预览改成“主线 1-7”之类的玩家可读关卡名，按关卡合并当前掉落并展示道具名、掉率与“约多少体力一个”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：官方公告目前仍展示全部公告，创作征集、制作组通讯等非活动内容尚未过滤；PRTS 仍未继续下钻到干员、配方、关卡静态映射等下一个结构化业务数据闭环；Penguin 的 stage 命名当前以规则映射为主，后续若要更自然可继续补 zone/title 映射
- 风险/阻塞：桌面端当前只支持固定时区与固定偏移值的显示转换，尚未引入 IANA 全量时区数据库；Penguin 的“当前掉落”判断目前只基于矩阵时间窗，不包含更高层的活动开放状态校验；官方公告过滤规则尚未落地，当前预览里仍可能出现用户不关心的公告
- 下一步：继续停留在 M3，回到 PRTS 的下一个结构化业务数据最小闭环，优先在配方或关卡静态映射里选择一个最小可写库路径；同步页仅做后续小幅收口，不再扩成大改版

### 变更记录

- 日期时间：2026-03-16 16:13:49 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：Penguin 当前掉落预览需要按关卡热度（最近上传量）排序，优先展示活动中掉落蓝色材料的关卡；掉落展示需要区分常规掉落和特殊掉落，额外物资默认折叠且不展示
- 新重要记忆：Penguin 的掉落类型不在 `result matrix` 里，而在 `stages.dropInfos` 中；实查 CN `stages` 接口后，当前实际值包括 `NORMAL_DROP` / `SPECIAL_DROP` / `EXTRA_DROP` / `FURNITURE`，因此预览层需要结合 stage 元数据做二次分组，不能只看矩阵本身
- 已完成：为 Penguin 预览查询补上 `stageType`、`dropType`、`item_type`、`rarity` 等展示所需字段；desktop 当前掉落预览改为优先展示活动中掉落蓝色材料的关卡，其余关卡再按最近上传量降序排列；同一关卡内拆分“常规掉落 / 特殊掉落”，并默认隐藏 `EXTRA_DROP` / 家具等额外物资；新增 desktop 单测覆盖“活动蓝材优先排序”和“特殊掉落分组且隐藏额外物资”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；PRTS 仍未推进到下一个结构化业务数据闭环；Penguin 当前预览的“蓝色材料”判断仍基于本地 rarity / item_type 规则，而不是专门的材料层级字典
- 风险/阻塞：`dropType` 依赖 `external_stage_def.raw_json.dropInfos` 与 `external_drop_matrix` 的 item 对齐；若 Penguin 后续调整 stage payload 结构，需要同步更新查询；当前预览只是在展示层折叠额外物资，并没有把该语义下沉成统一的规划层枚举
- 下一步：继续停留在 M3，停止追加 Penguin 展示层大改，回到 PRTS 的下一个结构化业务数据最小闭环，优先在配方或关卡静态映射里选一个继续推进

### 变更记录

- 日期时间：2026-03-16 16:18:38 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：检查并修正 Penguin 当前掉落预览里“把非正在进行中的活动也排进活动优先组”的问题；始终优先展示“正在进行中的活动中且掉落蓝色材料”的关卡，然后再展示当前可访问的全部关卡排序
- 新重要记忆：已用实时 Penguin CN `stages` 数据确认，活动关卡是否“正在进行中”必须看 `existence.CN.exist/openTime/closeTime`；仅看 `stageType = ACTIVITY` 或仅看矩阵时间窗都不够，旧活动如 `GT-1` 会因此误入活动优先组
- 已完成：为 Penguin 预览查询补上 `existence.CN.exist/openTime/closeTime`；当前掉落预览改为同时满足“关卡当前可访问 + 掉落窗口当前有效”才纳入展示；活动优先组收紧为“活动关卡 + CN 开放窗口当前有效 + 掉落蓝色材料”；新增 desktop 单测覆盖“已结束活动不会出现在当前概览中”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；PRTS 仍未推进到下一个结构化业务数据闭环；Penguin 当前预览的“蓝色材料”判断仍基于本地 rarity / item_type 规则，而不是专门的材料层级字典
- 风险/阻塞：当前“可访问”仍是基于 Penguin `stages` 的全局开放窗口，不包含玩家账号个人解锁进度；若 Penguin 后续调整 `existence` 字段结构，需要同步更新查询与过滤逻辑
- 下一步：继续停留在 M3，停止继续膨胀 Penguin 展示层，回到 PRTS 的下一个结构化业务数据最小闭环，优先在配方或关卡静态映射里选一个继续推进

### 变更记录

- 日期时间：2026-03-16 16:23:12 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：用户反馈当前掉落预览仍展示 `GT-5`、`TW-6`、`BI-6`、`RI-6` 等老活动关卡；需要进一步收紧过滤，始终优先展示“正在进行中的活动中且掉落蓝色材料”的关卡，然后才是当前可访问的其他关卡
- 新重要记忆：已用实时 Penguin CN `stages` 数据核对 `GT-5`、`TW-6`、`BI-6`、`RI-6`、`CW-10` 等典型旧活动，均为 `stageType = ACTIVITY` 且带已结束的 `closeTime`；同时确认常驻 `MAIN/DAILY` 通常只有 `exist = true`、没有时间窗。因此对于活动类关卡必须更保守：拿不到明确活动窗口就默认不展示，不能再当“当前掉落”
- 已完成：在 desktop 预览层新增“活动类关卡缺少明确窗口信息时直接隐藏”的保守过滤；`is_currently_accessible_penguin_drop` 现在会先判断该关卡是否需要显式活动窗口，再决定是否允许无 `openTime/closeTime` 的记录进入当前预览；新增 desktop 单测覆盖“像 `GT-5` 这类带活动语义但缺少窗口元数据的关卡不会进入当前概览”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；PRTS 仍未推进到下一个结构化业务数据闭环；Penguin 当前预览的“蓝色材料”判断仍基于本地 rarity / item_type 规则，而不是专门的材料层级字典
- 风险/阻塞：这次过滤是有意偏保守的：如果本地 stage 元数据不完整，某些真实正在进行中的活动关卡也可能先被隐藏，直到下一次 `sync penguin` 刷新出完整 `existence`；但这比继续把老活动误展示成“当前掉落”更符合当前要求
- 下一步：停止继续扩展 Penguin 展示规则，回到 M3 的 PRTS 下一个结构化业务数据最小闭环；若用户后续仍能贴出旧活动样例，再进一步检查本地数据库中 `external_stage_def.raw_json` 的历史残留形态

### 变更记录

- 日期时间：2026-03-16 16:31:00 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：用户进一步反馈新构建的 desktop 里仍能看到 `GT-5`、`TW-6`、`BI-6`、`RI-6`、`IW-6` 等活动关卡出现在当前掉落预览顶部，要求直接核实实际原因而不是继续猜测
- 新重要记忆：已通过临时探针直接查询当前 desktop 使用的 SQLite，确认这些关卡对应的是 `a001_05_perm`、`act11d0_06_perm`、`act14side_06_perm`、`act12d0_06_perm`、`act15side_06_perm` 一类 `*_perm` 常驻化活动关卡：它们 `stageType = ACTIVITY`，有 `openTime`，但没有 `closeTime`，因此属于“当前可访问的其他关卡”，而不属于“正在进行中的限时活动”
- 已完成：修正 Penguin 活动优先组判定：现在只有“活动关卡 + 存在明确 `closeTime` 且当前尚未结束 + 掉落蓝色材料”的关卡才会进入优先组；`*_perm` 常驻化活动仍可作为当前可访问关卡进入第二组，但不会再压过真正正在进行中的活动；新增 desktop 单测覆盖“`*_perm` 活动关卡可访问但不属于优先组”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已重新执行 `cargo build -p akbox-desktop --release` 生成新的 release 可执行文件
- 未完成：官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；PRTS 仍未推进到下一个结构化业务数据闭环；同步页当前还没有把“优先组 / 其他可访问关卡”显式分段显示，用户只能从排序上感知
- 风险/阻塞：如果用户继续使用旧进程或旧产物，界面仍会保持旧排序；另外，`*_perm` 是否应完全隐藏而不是放在第二组，当前仍按“当前可访问的其他关卡”处理，后续若产品口径改变还需再收口
- 下一步：若用户确认最新 release 里第二组仍需要继续裁剪，则把 Penguin 预览从单表改成“正在进行中的活动关卡 / 其他当前可访问关卡”两段式展示；否则按既定路线回到 PRTS 下一个结构化业务数据最小闭环

### 变更记录

- 日期时间：2026-03-16 16:34:56 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：用户反馈当前掉落预览里还出现了“公开招募 (公开招募) 99 理智 982438935 次”，要求检查原因并移除；该条目不应被当作刷图关卡展示
- 新重要记忆：已直接核对 Penguin 实时 `stages` 数据，确认其确实存在 `stageId = recruit`、`zoneId = recruit`、`code = 公开招募`、`stageType = MAIN`、`apCost = 99` 的伪关卡，并把公开招募标签当作 `NORMAL_DROP`；因此预览层不能只依赖 `stageType` / `existence`，还必须显式排除这类非战斗 stage
- 已完成：在当前掉落预览过滤层新增对 `recruit / 公开招募` 伪关卡的硬排除；新增 desktop 单测覆盖“公开招募不会进入当前掉落概览”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已重新执行 `cargo build -p akbox-desktop --release`，生成最新的 [target/release/akbox-desktop.exe]
- 未完成：官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；PRTS 仍未推进到下一个结构化业务数据闭环；同步页当前还没有把“优先组 / 其他当前可访问关卡”显式分段显示，用户只能从排序上感知
- 风险/阻塞：当前对伪关卡的过滤先走显式规则（`recruit / 公开招募`），后续若 Penguin 再暴露其他非战斗 stage，还需要继续补专门规则，而不是期待靠通用字段自然排除
- 下一步：若用户继续发现其他明显不应展示的伪关卡，就把 Penguin 预览的“可展示 stage”抽成更明确的白名单 / 黑名单规则；否则按既定路线回到 PRTS 下一个结构化业务数据最小闭环

### 变更记录

- 日期时间：2026-03-16 17:03:35 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：继续推进 PRTS 主线；desktop 中 PRTS 的多个同步按钮需要收敛成一个“同步 PRTS 全部”按钮，点一次顺序同步当前所有 PRTS 内容
- 新重要记忆：PRTS 的 desktop 入口后续默认走“单按钮全量同步”，避免随着结构化子源增加继续膨胀成多按钮；本轮仍按最小闭环推进，先做 `配方 -> external_recipe`，再把现有 siteinfo / item / stage / recipe 串成统一后台任务
- 已完成：已先把新需求和下一步写回 `AGENTS.md`，避免后续代码改动脱离项目长期记忆
- 未完成：`external_recipe` 仍未开始同步；desktop 仍是多个 PRTS 按钮；还没有检查本轮将使用的 PRTS 配方数据源
- 风险/阻塞：若 PRTS 配方源字段结构和道具/关卡不同，可能需要额外做字段归一或分页处理；若直接把多个子同步串起来，还要注意 GUI 文案和失败提示不能丢失具体来源
- 下一步：检查现有 PRTS / repository / desktop 代码，确定 `external_recipe` 的最小数据源和单按钮全量同步的实现落点

### 变更记录

- 日期时间：2026-03-16 17:00:08 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：继续严格按既定最小闭环推进，优先完成 `PRTS 关卡静态映射 -> external_stage_def`
- 新重要记忆：PRTS 的关卡静态映射当前通过 `action=parse&page=关卡一览&prop=revid&format=json` 获取 revision，再通过 `action=ask&query=[[关卡id::+]]|?关卡id|?分类|limit=500[|offset=n]&format=json` 分页拉取；为避免破坏现有 Penguin 关卡展示，`external_stage_def.raw_json` 现在保留 Penguin 根对象，并把 PRTS 负载挂到 `$.prts`
- 已完成：在 `crates/akbox-data::prts` 中新增 PRTS 关卡索引分页拉取与解析、`PrtsStageDefinition` / `PrtsStageIndexResponse`、revision 获取与真实 URL 构造；在 repository 中新增 `external_stage_def` 的 PRTS upsert / count / list，并修正 Penguin stage upsert 会保留既有 `$.prts`；在同步骨架中新增 `sync_prts_stage_index`、`PRTS_STAGE_INDEX_SOURCE_ID = prts.stage-index.cn`、`PRTS_STAGE_INDEX_CACHE_KEY = prts:stage-index:cn`，并补齐失败告警写入与恢复；为 CLI 增加 `sync prts-stages [database_path]`；为 desktop 的 `PRTS` 标签增加“同步 PRTS 关卡”按钮，并同时展示站点信息、道具索引与关卡索引状态、数量和样例；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -q -p akbox-cli -- sync prts-stages C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-prts-stages-2cda36c4-ae0a-457d-91f8-ac471e9f00f3\test.db`，成功写入 `3237` 条关卡定义，`Revision` 为 `375661`，缓存字节数为 `2296363`
- 未完成：PRTS 仍未同步配方、干员基础资料、养成需求与基建技能；官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；desktop 同步页当前仍以摘要表格为主，尚未提供更细粒度进度回传
- 风险/阻塞：当前 PRTS 关卡同步的 `zone_id` 仍主要依赖后续 Penguin 或更细的 PRTS 数据补齐；现阶段已保证不会覆盖 Penguin 的 stage 根 payload，但更丰富的分区/章节语义仍未结构化拆列；`action=ask` 的字段命名和分类体系若后续调整，需要同步更新解析逻辑
- 下一步：继续停留在 M3，优先完成 `PRTS 配方 -> external_recipe` 的最小闭环；若期间用户继续反馈 Penguin 伪关卡或活动过滤异常，再以小补丁回收展示层规则

### 变更记录

- 日期时间：2026-03-16 17:21:41 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：继续推进 PRTS 主线；desktop 里的 PRTS 多按钮需要收敛成一个“同步 PRTS 全部”按钮，点击后顺序同步当前所有 PRTS 内容
- 新重要记忆：PRTS 配方当前通过 MediaWiki API `action=parse&page=罗德岛基建/加工站&prop=revid|text&format=json` 获取并解析加工站表；由于实时页面存在相同产物/等级的重复配方行，`external_recipe.recipe_id` 当前按 `workshop:{output_item_id}:lv{level}:row{n}` 生成，并在每次同步时全量替换；CLI `sync prts` 与 desktop“同步 PRTS 全部”现在语义一致，都会顺序执行 siteinfo / item / stage / recipe 四段同步
- 已完成：在 `crates/akbox-data::prts` 中补齐 `PrtsRecipeIndexResponse` / `PrtsRecipeDefinition` 与加工站配方表解析；在 repository 中新增 `external_recipe` 的 replace / count / list，以及按中文名反查 `external_item_def` 的能力；在同步骨架中新增 `sync_prts_recipe_index`、`PRTS_RECIPE_INDEX_SOURCE_ID = prts.recipe-index.cn`、`PRTS_RECIPE_INDEX_CACHE_KEY = prts:recipe-index:cn`，并补齐道具名到 `item_id` 的解析、失败告警与原始缓存写入；新增 CLI `sync prts-recipes [database_path]`，并把 `sync prts [database_path]` 改为全量 PRTS 同步；desktop 的 `PRTS` 标签新增配方概览，并将原有多个 PRTS 按钮收敛为单个“同步 PRTS 全部”按钮；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -q -p akbox-cli -- sync prts C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-prts-all-e923900f-b84c-4d38-896b-015399aba34a\test.db`，成功写入 `1227` 条道具、`3237` 条关卡、`66` 条配方，站点时间为 `2026-03-16T09:21:10Z`，配方 revision 为 `342715`
- 未完成：PRTS 仍未推进到干员基础资料、养成需求与基建技能；官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；desktop 还没有对 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：当前配方同步依赖先前已同步的 `external_item_def` 做中文名到 `item_id` 的匹配；虽然全量 `sync prts` 已自动先跑 item，再跑 recipe，但若后续单独执行 `sync prts-recipes` 且本地缺少对应 item 定义，仍会按预期失败并写告警；另外，配方解析仍依赖 PRTS 当前加工站表格结构，若页面列结构调整，需要同步更新 HTML 解析逻辑
- 下一步：继续停留在 M3，优先在 PRTS 的干员基础资料、养成需求或基建技能里挑一个新的最小结构化闭环；desktop 同步页仅做小幅补充，不继续膨胀成复杂任务中心

### 变更记录

- 日期时间：2026-03-16 17:27:58 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：修复 desktop “同步 PRTS 全部”在现有库上因同名道具歧义而失败的问题；当前已知样例为配方里的 `碳` 同时匹配 `200008` 与 `3112`
- 新重要记忆：PRTS 配方同步不能把“同名即报错”作为长期策略；在与 Penguin 共库时，会出现同名别名/旧 id 与 PRTS 正式定义并存的情况，配方解析需要优先选可判定的正式定义，再把无法判定的少数情况保留为真正错误
- 已完成：已先将该需求写回 `AGENTS.md`，准备按最小补丁检查 `external_item_def` 同名候选的来源差异，并为 `resolve_external_item_id_by_name` 增加稳定的优先级规则
- 未完成：尚未确认 `3112` 的来源形态，也尚未落地新的同名道具解析规则；当前 desktop 上的 PRTS 全量同步仍可能因该歧义失败
- 风险/阻塞：如果后续发现某些同名候选都来自 PRTS 正式定义，而不是 Penguin 别名/旧 id，简单“优先 PRTS”规则仍可能不够，需要继续补更细的判定字段
- 下一步：检查 `碳` 的重复定义来源，优先落地“同名时优先 PRTS 正式定义、其次优先 Penguin canonical sortId”的小修补，并重新验证 `sync penguin` 后再 `sync prts` 的混合同步路径

### 变更记录

- 日期时间：2026-03-16 17:27:49 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：修复 desktop “同步 PRTS 全部”在现有库上因同名道具歧义而失败的问题；当前已知样例为配方里的 `碳` 同时匹配 `200008` 与 `3112`
- 新重要记忆：已确认 `3112` 不是 PRTS 正式 item id，而是 Penguin 侧的同名旧 id / 别名项，且其 `sortId = 200008`；因此配方解析时同名冲突应优先落到 PRTS 正式定义，再把 Penguin alias 当作降级候选，而不是直接报错
- 已完成：在 `repository` 中新增按中文名读取 item 候选及其 `has_prts_payload / penguin.sortId / penguin.groupID` 元数据；在 `sync` 的 `resolve_external_item_id_by_name` 中新增“优先 PRTS 正式定义、其次优先 `item_id == sortId` 的 Penguin canonical 项”的打分规则；新增单测覆盖 `碳` 在 `200008` 与 `3112` 共存时会稳定选择 `200008`；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行“先 `cargo run -q -p akbox-cli -- sync penguin <tempdb>`，再 `cargo run -q -p akbox-cli -- sync prts <tempdb>`”复现路径，确认混合同库下 `PRTS full sync succeeded`，不再因 `碳` 歧义失败
- 未完成：PRTS 仍未推进到干员基础资料、养成需求与基建技能；官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；desktop 还没有对 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：当前同名解析规则已覆盖“PRTS 正式定义 + Penguin alias”这类主流场景，但如果后续出现“多个候选都带 PRTS 正式负载”或“多个候选都像 canonical”的情况，仍会保守报歧义错误，需要再引入更细的业务语义字段
- 下一步：继续停留在 M3，回到 PRTS 主线，从干员基础资料、养成需求或基建技能里挑一个新的最小结构化闭环；若用户再反馈其他同名 item 解析冲突，再把当前优先级规则继续扩成更明确的 canonical 归一逻辑

### 变更记录

- 日期时间：2026-03-16 17:28:53 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：无
- 新重要记忆：desktop 修复这种同步问题后，除代码验证外还需要补一次新的 release 构建，避免用户继续启动旧 exe 误判“修复无效”
- 已完成：在完成 `碳 -> 200008 / 3112` 歧义修补后，补执行 `cargo build -p akbox-desktop --release`，确认新的 release 桌面产物已成功生成
- 未完成：PRTS 仍未推进到干员基础资料、养成需求与基建技能；官方公告仍未过滤掉创作征集、制作组通讯等非活动公告；desktop 还没有对 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：如果用户仍在运行旧进程或旧桌面快捷方式，即使修补和 release 构建都已完成，界面上仍会继续看到旧行为；需要确保实际启动的是本次新构建
- 下一步：继续停留在 M3，回到 PRTS 主线，从干员基础资料、养成需求或基建技能里挑一个新的最小结构化闭环；若用户再次反馈同步问题，先确认运行的是本次新 release

### 变更记录

- 日期时间：2026-03-16 17:32:20 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：按顺序继续推进 PRTS 主线：1. `external_operator_def`；2. `external_operator_growth`；3. `external_operator_building_skill`
- 新重要记忆：虽然用户已经把后续三步顺序定下来了，但当前仍按 AGENTS 的“一个最小闭环”推进；本轮只先落 `PRTS 干员基础资料 -> external_operator_def`，确认数据源、写库与 GUI 摘要稳定后，再进入 growth 和 building skill
- 已完成：已先将新的顺序要求写回 `AGENTS.md`，并确认本地 schema 已预留 `external_operator_def / external_operator_growth / external_operator_building_skill`
- 未完成：尚未确认 PRTS 干员基础资料的最小 API / 页面锚点；`external_operator_def` 还没有 repository、sync、CLI、desktop 支撑；growth 和 building skill 也尚未开始
- 风险/阻塞：如果直接把 1/2/3 一次性并进全量同步，会违反当前“一次只做一个最小闭环”的约束，也会放大 PRTS 字段探测的不确定性
- 下一步：先只实现 `PRTS 干员基础资料 -> external_operator_def`，打通数据源、写库、CLI `sync prts-operators`、desktop 概览和 `sync prts` 聚合入口；完成并验证后，再进入 `external_operator_growth`

### 变更记录

- 日期时间：2026-03-16 17:41:23 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：继续按既定顺序推进 PRTS 主线，但本轮只做一个最小闭环：`PRTS 干员基础资料 -> external_operator_def`
- 新重要记忆：PRTS 干员基础资料当前通过两步 MediaWiki API 组合获取：先用 `action=parse&page=干员一览&prop=revid&format=json` 取 revision，再用 `action=ask&query=[[干员id::+]]|?干员id|?稀有度|?职业|?分支|limit=500[|offset=n]&format=json` 分页拉取结构化干员索引；同步入口为 `akbox-cli sync prts-operators [database_path]`，成功时会写入 `raw_source_cache(cache_key = prts:operator-index:cn)`、更新 `sync_source_state(source_id = prts.operator-index.cn)`，并 upsert 到 `external_operator_def`
- 已完成：在 `crates/akbox-data::prts` 中补齐 `PrtsOperatorIndexResponse` / `PrtsOperatorDefinition` 与分页 `ask` 解析；在 repository 中新增 `external_operator_def` 的 count / list / upsert；在同步骨架中新增 `sync_prts_operator_index`、`PRTS_OPERATOR_INDEX_SOURCE_ID = prts.operator-index.cn`、`PRTS_OPERATOR_INDEX_CACHE_KEY = prts:operator-index:cn`，并把 `sync prts` 扩成 siteinfo / operator / item / stage / recipe 五段同步；新增 CLI `sync prts-operators [database_path]`；desktop 的 `PRTS` 标签新增干员索引概览，`同步 PRTS 全部` 完成后会回显干员数量；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -q -p akbox-cli -- sync prts-operators C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-prts-operators-830a647f-eccf-432d-b922-867488bf63af\test.db`，成功写入 `438` 条干员定义，revision 为 `335492`，缓存 `100396` 字节；并执行 `cargo run -q -p akbox-cli -- sync prts C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-prts-all-operators-7c7e4413-184a-4f8b-9d04-0a56064c7b9a\test.db`，确认全量同步也已带上干员索引；另已执行 `cargo build -p akbox-desktop --release` 生成新的 desktop release 产物
- 未完成：`external_operator_growth` 与 `external_operator_building_skill` 仍未开始；官方公告仍未过滤创作征集、制作组通讯等非活动内容；desktop 还没有对 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：当前干员基础资料只同步了 `operator_id / 名称 / 稀有度 / 职业 / 分支` 这组最小字段；如果后续要补职业分支、子职业或头像等更细资料，可能需要切换到单干员页模板或补额外 `ask` 字段；同时，PRTS 实时 `干员一览` 当前只返回 `438` 条定义，后续若页面字段或分页策略调整，需要同步更新解析逻辑
- 下一步：继续停留在 M3，按既定顺序进入 `PRTS 养成需求 -> external_operator_growth`；仍保持一个最小闭环，先确定最小可稳定抓取的数据源和字段，再接 repository / sync / desktop 聚合入口

### 变更记录

- 日期时间：2026-03-16 17:51:26 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：`external_operator_def` 里不应保留玩家 box 无法拥有的临时干员（如 `Mechanist(卫戍协议)`）；需要先检查 PRTS 是否已有稳定标记，对这类干员做分类，并从干员定义同步结果中移除
- 新重要记忆：这轮不是直接进入 `external_operator_growth`，而是先回补 `external_operator_def` 的“可拥有性”边界，避免后续 growth / building skill 把模式临时干员一并带入本地定义库
- 已完成：已先将该需求写回 `AGENTS.md`，准备检查 PRTS 的 `干员一览` / 单干员页 / 分类字段里是否已有可稳定识别“临时干员 / 不可入 box”的标记
- 未完成：尚未确认 PRTS 的具体标记字段，也尚未落地分类和同步过滤规则；当前 `external_operator_def` 仍可能包含 `Mechanist(卫戍协议)` 这类非 box 干员
- 风险/阻塞：如果 PRTS 没有直接字段，只能依赖分类名或页面名模式做识别，规则需要尽量保守，避免误删未来可能可获取的正式干员变体
- 下一步：先验证 PRTS 是否存在稳定分类或 printout 字段可区分“玩家可拥有干员 / 临时模式干员”，再以最小补丁把分类规则纳入 `sync_prts_operator_index`，并重新验证单独同步与全量同步路径

### 变更记录

- 日期时间：2026-03-16 17:57:06 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：`external_operator_def` 必须剔除玩家 box 无法拥有的临时干员；已知样例包括 `Mechanist(卫戍协议)`，要求先利用 PRTS 原生标记分类，再把这类条目从干员定义同步结果中移除
- 新重要记忆：已用实时 PRTS API 核实，`Mechanist(卫戍协议)`、`暮落(集成战略)`、`Sharp`、`Touch`、`Stormeye`、`郁金香`、各类预备干员等模式专属条目都会落在 `分类:专属干员`；而 `阿米娅(近卫)` 这类玩家可拥有的正式变体不在该分类内。因此当前可把 `专属干员` 视为“非 box、不可作为看号养成基线”的稳定过滤标记
- 已完成：在 `crates/akbox-data::prts` 的干员解析中补充 `?分类` printout，并把 `categories / availability_kind / is_box_collectible` 写入干员原始 JSON；在 `sync_prts_operator_index` 中按 `is_box_collectible` 过滤条目；为确保旧库里的临时干员也会被清掉，在 repository 中新增 `replace_external_operator_defs` 并将干员同步改为替换写入；补充单测覆盖“`专属干员` 不入库且旧残留会被删除”；desktop 的 PRTS 概览文案也已注明“`专属干员` 会被过滤”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -q -p akbox-cli -- sync prts-operators C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-prts-operator-filter-check\test.db`，当前写入 `410` 条干员定义，并确认 `char_610_acfend` 已不存在；另已执行 `cargo build -p akbox-desktop --release` 生成新的 desktop release 产物
- 未完成：`external_operator_growth` 与 `external_operator_building_skill` 仍未开始；官方公告仍未过滤创作征集、制作组通讯等非活动内容；desktop 还没有对 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：当前过滤规则依赖 PRTS 继续稳定维护 `分类:专属干员` 这一语义；如果后续编辑策略调整、把某些正式可拥有干员也并入该分类，或改用其他分类名承载模式专属语义，需要重新核验并同步更新规则
- 下一步：继续停留在 M3，回到既定顺序进入 `PRTS 养成需求 -> external_operator_growth`；仍保持一个最小闭环，先确认最小字段集与稳定 API，再接 repository / sync / desktop 聚合入口

## 26. 首次编码默认顺序（不要跳）

第一次真正写代码时必须按下面顺序：
1. Workspace 与 rust-toolchain
2. desktop 空窗体
3. cli 空命令
4. config
5. logging
6. sqlite + migrations
7. 审计日志表
8. 再开始外部同步和 ADB

## 27. 如果出现新需求，默认处理原则

- 先吸收进 AGENTS.md
- 再判断属于哪个阶段
- 若会影响架构，先更新“关键设计决策”
- 若不影响架构，放入对应阶段待办
- 不因单个新需求推翻整个已完成阶段，除非用户明确要求重构

## 28. 结束语

本文件是项目长期记忆与执行准绳。
Codex 不得假设“记在上下文里就够了”。
只要发生了需求变化、重要决定、完成进度变化，就必须把它写回本文件。

### 变更记录

- 日期时间：2026-03-16 20:24:21 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：将当前这一轮 M3 的累计改动整理成多个清晰 commit，并推送到远端分支
- 新重要记忆：这一步属于版本整理与交付，不新增功能；提交拆分需要按主题边界组织，而不是把当前工作区直接做成单个大提交
- 已完成：已确认当前工作区存在一整段未提交的 M3 累计改动，涉及 `official / penguin / prts / sync / repository / desktop / cli / AGENTS` 等文件；当前分支为 `master`，远端为 `origin`
- 未完成：尚未完成提交拆分、提交说明与远端推送
- 风险/阻塞：若直接一次性提交，会丢失“官方公告 / PRTS 结构化同步 / 增量同步与 UI 收口”这些主题边界；需要先梳理 diff，再按逻辑分组提交
- 下一步：检查 diff 并设计最小可读的 commit 切分方案，然后分批 `git add` / `git commit`，最后推送到 `origin/master`

- 日期时间：2026-03-16 20:04:32 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：`PRTS 养成需求` 预览里的通用技能升级聚合行，不能再展示 `1→2：...；2→3：...` 这类逐级明细；对玩家应直接展示各材料的需求总数，如 `技巧概要·卷1 xN / 技巧概要·卷2 xN`
- 新重要记忆：desktop 的 `PRTS 养成需求` 预览现在仍保留 `1→7` 这类阶段标签，但该行内部材料明细已固定为“按材料名汇总总数”的玩家视角展示；`external_operator_growth` 底层继续保留逐级材料，不改变写库粒度
- 已完成：在 `apps/akbox-desktop` 的 `build_prts_operator_growth_display_rows` 中，将通用技能升级聚合行从“逐级前缀串接”改为“按材料名累加总数”展示；新增 `parse_growth_material_segment` 与 `summarize_growth_material_totals`，会把诸如 `技巧概要·卷1 x5 / 破损装置 x4` 这类片段解析后跨多级合并成总量字符串；更新 desktop 单测，覆盖 `技巧概要·卷1 / 卷2 / 卷3` 与多种材料会在 `1→7` 聚合行中正确汇总、且不再出现 `1→2：` 前缀；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；另已执行 `cargo build -p akbox-desktop --release` 生成新的 desktop release 产物
- 未完成：`external_operator_building_skill` 仍未开始；官方公告仍未过滤创作征集、制作组通讯等非活动内容；当前 `PRTS growth` 仍缺稳定轻量增量锚点
- 风险/阻塞：当前总量汇总仍依赖展示层解析 `材料名 x数量` 字符串；若后续 `material_summary` 的格式改变，需同步调整解析函数；此外，非 `通用 1→7` 的其他养成行目前仍按原始摘要直接展示
- 下一步：继续停留在 M3，按既定顺序进入 `PRTS 基建技能 -> external_operator_building_skill`；若继续打磨养成需求展示，再评估是否对精英化 / 专精行也增加总量汇总视图

- 日期时间：2026-03-16 20:03:06 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：`PRTS 养成需求` 预览里的通用技能升级聚合行，不能再展示 `1→2：...；2→3：...` 这类逐级明细；对玩家应直接展示各材料的总需求，如 `技巧概要·卷1 xN / 技巧概要·卷2 xN`
- 新重要记忆：这条仍然只改 desktop 展示层，不改 `external_operator_growth` 的底层逐级存储；当前“通用 1→7”聚合行后续应以“按材料名汇总总数”的方式展示，而不是保留每一级来源
- 已完成：已确认现有 `build_prts_operator_growth_display_rows` 只是把 `1→2 ... 6→7` 用分号串起来，尚未做材料总量汇总；准备直接在该聚合函数里改成按材料名求和，并同步更新单测
- 未完成：代码尚未修改；养成需求预览里仍会出现对玩家无用的逐级前缀信息
- 风险/阻塞：当前材料摘要来自字符串而非结构化列，展示层汇总需要先解析 `材料名 x数量` 片段；若后续摘要格式变化，需要同步调整解析函数
- 下一步：在 desktop 的 `build_prts_operator_growth_display_rows` 中把通用技能升级聚合行改为材料总量汇总展示，并补单测覆盖“同名材料跨多级会正确累加”，然后执行 fmt / clippy / test 并回写本文件

- 日期时间：2026-03-16 19:59:25 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：将 `PRTS 养成需求` 从当前聚合入口中拆成独立同步入口；desktop 需要新增“同步 PRTS 养成需求”按钮，原“同步 PRTS 全部”改名为“同步 PRTS”，且新的“同步 PRTS”不再触发 `PRTS growth`
- 新重要记忆：当前用户口径里的“同步 PRTS”已经固定为常规五段同步：`siteinfo / operator / item / stage / recipe`；`PRTS growth` 继续保留独立入口 `sync prts-growth` 与 desktop“同步 PRTS 养成需求”，避免把高耗时、暂不支持安全增量的养成需求继续混进常规同步
- 已完成：在 `akbox-data::sync` 中将 PRTS 聚合入口重构为不含 `growth` 的 `sync_prts` / `sync_prts_with_mode`，并保留旧 `sync_prts_all*` 兼容别名但同样只执行常规五段同步；`SyncPrtsOutcome` 现仅回报站点 / 干员 / 道具 / 关卡 / 配方，不再包含养成需求；CLI `sync prts [--full]` 的帮助文案与输出已同步改为“不含 growth”，`sync prts-growth [--full]` 继续独立可用；desktop 同步页已将原按钮改名为“同步 PRTS”，新增“同步 PRTS 养成需求”按钮，并为养成需求补充独立后台任务、结果提示与状态文案；同步页与 PRTS 概览文案已明确说明“常规 PRTS 同步”和“养成需求同步”是两个入口；同步层单测已改为验证 `sync prts` 只执行 siteinfo / operator / item / stage / recipe，且不会写入 `external_operator_growth`；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；另已执行 `cargo build -p akbox-desktop --release` 生成新的 desktop release 产物
- 未完成：`external_operator_building_skill` 仍未开始；官方公告仍未过滤创作征集、制作组通讯等非活动内容；当前 `PRTS growth` 仍缺稳定轻量增量锚点
- 风险/阻塞：`sync_prts_all*` 当前为了兼容旧调用仍保留为别名，但语义已经变成“不含 growth”；后续若继续清理命名，需要统一移除历史 `all` 命名，避免新的调用方误解
- 下一步：继续停留在 M3，按既定顺序进入 `PRTS 基建技能 -> external_operator_building_skill`；若继续打磨同步体验，优先考虑把旧 `sync_prts_all*` 命名彻底收敛掉，再评估 `PRTS growth` 的安全增量发现机制

- 日期时间：2026-03-16 19:44:05 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：将 `PRTS 养成需求` 从当前聚合入口中拆成独立同步入口；desktop 需要新增“同步 PRTS 养成需求”按钮，原“同步 PRTS 全部”改名为“同步 PRTS”，且新的“同步 PRTS”不再触发 `PRTS growth`
- 新重要记忆：从这一步开始，用户口径里的“同步 PRTS”仅指 `siteinfo / operator / item / stage / recipe` 五段常规同步；`external_operator_growth` 改为显式单独触发，避免把高耗时、仍缺稳定增量锚点的养成需求继续混在常规同步入口里
- 已完成：已确认当前 `sync_prts_all_with_mode`、CLI `sync prts` 与 desktop“同步 PRTS 全部”仍会串上 `growth`，需要同步调整数据层聚合函数、CLI 帮助文案与 desktop 按钮/任务模型
- 未完成：代码尚未修改；`sync prts` 仍包含 `growth`，desktop 也还没有独立的“同步 PRTS 养成需求”按钮
- 风险/阻塞：如果只改 UI 文案而不改底层聚合函数，会继续造成 `sync prts` 语义与实际执行不一致；需要把 CLI、desktop 和 `akbox-data::sync` 一起收口
- 下一步：将 PRTS 聚合同步从“含 growth”改为“常规五段同步”，保留 `sync prts-growth` 独立入口，并在 desktop 同步页新增单独按钮和对应状态提示；完成后执行 fmt / clippy / test 并回写本文件

- 日期时间：2026-03-16 18:54:07 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：继续按既定顺序推进 `PRTS 养成需求 -> external_operator_growth`；同时修复真实同步中暴露出的两类边界问题：`预备干员-*` 这类非玩家 box 干员不能进入养成需求同步，且已有 `external_operator_growth` 数据时再次全量同步 `sync prts` 不能因 `external_operator_def` 的替换写入触发外键失败
- 新重要记忆：PRTS 的“非玩家 box 干员”不只依赖 `分类:专属干员`；实时数据里 `预备干员-*` 没有 `专属干员` 分类，但页面标题前缀稳定为 `预备干员`，因此当前以“`专属干员` 分类 + `预备干员` 页面标题前缀”共同判定不可入 box；另外，`external_operator_def` 进入有下游外键依赖阶段后，替换同步不能再用“全表 DELETE 后重插”策略，必须先 upsert，再删除真正过时的干员及其依赖行
- 已完成：在 `crates/akbox-data::prts` 中新增 `fetch_operator_growth`，通过 `action=parse&page=<干员页>&prop=sections|revid|text&section=n&format=json` 拉取“精英化材料 / 技能升级材料”两个 section，解析并写入 `external_operator_growth`；为 PRTS 原始请求补充 3 次有限重试，覆盖实时 `504 Gateway Time-out` 抖动；在 repository 中新增 `external_operator_growth` 的 replace / count / list，并把 `replace_external_operator_defs` 改为“先 upsert，再删除 stale operator / stale growth / stale building_skill”，修复已有 growth 数据时 `sync prts` 的外键失败；在同步骨架中新增 `sync_prts_operator_growth`、`PRTS_OPERATOR_GROWTH_SOURCE_ID = prts.operator-growth.cn`、`PRTS_OPERATOR_GROWTH_CACHE_KEY = prts:operator-growth:cn`，并把 `sync prts` 扩成 siteinfo / operator / item / growth / stage / recipe 六段同步；新增 CLI `sync prts-growth [database_path]`；desktop 的 `PRTS` 概览新增养成需求状态与样例，并把干员过滤说明更新为“`专属干员` 与 `预备干员` 会被过滤”；新增测试覆盖 `预备干员` 过滤、growth 同步、以及 operator replace 在存在 growth 行时仍能删除 stale 干员；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -q -p akbox-cli -- sync prts-growth C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-prts-growth-e7f70ee0-8ace-4399-91a8-135361d0a519\test.db`，成功写入 `5774` 条养成需求，revision 为 `385847`；随后在同库执行 `cargo run -q -p akbox-cli -- sync prts C:\Users\EMMMER~1.SUP\AppData\Local\Temp\arkagent-prts-growth-e7f70ee0-8ace-4399-91a8-135361d0a519\test.db` 也成功，当前全量同步结果为 `409` 条干员、`1227` 条道具、`5774` 条养成需求、`3237` 条关卡、`66` 条配方；另已执行 `cargo build -p akbox-desktop --release` 生成新的 desktop release 产物
- 未完成：`external_operator_building_skill` 仍未开始；官方公告仍未过滤创作征集、制作组通讯等非活动内容；desktop 还没有对 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：当前养成需求解析依赖 PRTS 单干员页里“精英化材料 / 技能升级材料” section 标题与表格结构保持稳定；如果后续 section 名或 HTML 结构调整，需要同步更新解析逻辑；此外，真实 `sync prts-growth` 请求量较大，虽然已补有限重试，但单次全量同步耗时仍偏长
- 下一步：继续停留在 M3，按既定顺序进入 `PRTS 基建技能 -> external_operator_building_skill`；仍保持一个最小闭环，先确定最小稳定字段与页面锚点，再接 repository / sync / desktop 聚合入口

### 变更记录

- 日期时间：2026-03-16 19:17:50 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：养成需求展示面板里，通用技能升级的 `1→2 ... 6→7` 不应逐行平铺；要求在展示层整合整理成 `1→7`
- 新重要记忆：这条是 desktop 预览层的收口，不改变 `external_operator_growth` 的底层存储粒度；同步数据仍保留逐级材料，面板只负责把玩家视角里碎片化的技能升级步骤聚合显示
- 已完成：已先将该展示需求写回 `AGENTS.md`，准备检查 desktop 当前 `PRTS` 养成需求预览的渲染逻辑，并在不改写库结构的前提下追加聚合显示
- 未完成：尚未落地 `1→7` 的聚合显示，也尚未补对应 desktop 单测；`external_operator_building_skill` 仍未开始
- 风险/阻塞：如果直接改 repository 查询或写库结构，会扩大改动面并偏离“只改展示层”的最小闭环；需要把聚合压在 desktop 预览层完成
- 下一步：在 desktop 的 PRTS 养成需求预览中新增通用技能升级聚合逻辑，把同一干员的 `1→2 ... 6→7` 合并成 `1→7`，然后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-16 19:19:43 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：养成需求展示面板里，通用技能升级的 `1→2 ... 6→7` 需要在玩家视角整合成 `1→7`
- 新重要记忆：`external_operator_growth` 继续保留逐级材料明细；desktop 预览层现在会把同一干员 `material_slot = 通用` 的连续技能升级步骤聚合为一条展示记录，并把每一级材料以前缀 `1→2：...；2→3：...` 的方式串到同一行里，避免同步摘要面板被碎片化行数淹没
- 已完成：在 desktop 的 `PRTS` 养成需求预览中新增 `build_prts_operator_growth_display_rows` 聚合逻辑，把同一干员的通用技能升级从 `1→2 ... 6→7` 合并显示为 `1→7`；为避免只抓到半段数据，养成需求预览的样例读取上限从 `8` 行提高到 `24` 行，再在展示层取前 `8` 条聚合结果；新增 desktop 单测覆盖“`1→2 ... 6→7` 会被合并为 `1→7` 且专精行保持独立”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；另已执行 `cargo build -p akbox-desktop --release` 生成新的 desktop release 产物
- 未完成：`external_operator_building_skill` 仍未开始；官方公告仍未过滤创作征集、制作组通讯等非活动内容；desktop 还没有对 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：当前这次收口仅覆盖 `material_slot = 通用` 且阶段标签可解析为 `n→n+1` 的技能升级行；如果后续用户还希望把精英化、专精或其他材料段继续折叠成更高层级摘要，需要再定义新的聚合展示规则
- 下一步：继续停留在 M3，回到既定顺序进入 `PRTS 基建技能 -> external_operator_building_skill`；若用户继续细化养成需求面板，再以展示层最小补丁收口，不回退底层存储粒度

### 变更记录

- 日期时间：2026-03-16 19:22:31 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：所有同步功能需要支持“默认增量、勾选全部同步时强制全量”的模式切换；本轮先接 `PRTS item/operator/stage/recipe` 与 `Penguin`，`官方公告` 和当前 `PRTS growth` 先保持全量，并在 UI 中明确说明原因；若某个源没有稳定 API 级增量能力，则不强行实现
- 新重要记忆：这轮的“增量同步”定义不是字段级 patch，而是“先用轻量版本锚点预检查，未变化则跳过重抓；选择全量时无条件执行原有完整同步”；当前已确认 `PRTS item/operator/stage/recipe` 可基于 `revid` 预检查，`Penguin` 可基于 `HEAD` 返回的 `Last-Modified` 做源级跳过；`官方公告` 与当前 `PRTS growth` 暂无稳定增量锚点
- 已完成：已先将这条模式切换需求写回 `AGENTS.md`，准备在不改动既有表结构语义的前提下，为 sync 层增加统一的 `增量 / 全量` 模式入口，并把 desktop / CLI 的同步操作切到默认增量
- 未完成：代码尚未开始；`增量 / 全量` 模式、PRTS/Penguin 的预检查跳过逻辑、desktop 全量开关与相关文案都还没有落地
- 风险/阻塞：`PRTS growth` 当前来自大量单干员 section 页面，尚不能安全用单一 revision 判定“全体未变化”；如果硬做伪增量，容易漏掉单页更新；因此本轮只能明确保持全量
- 下一步：先实现统一的同步模式枚举和结果回传，再接入 `PRTS item/operator/stage/recipe` 与 `Penguin` 的预检查跳过；随后更新 CLI / desktop 的入口、提示文案和验证用例，并回写本文件

### 变更记录

- 日期时间：2026-03-16 19:36:12 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：所有同步功能默认走增量；用户勾选“全部同步”时才强制全量。本轮先接 `PRTS item/operator/stage/recipe` 与 `Penguin`，`官方公告` 和当前 `PRTS growth` 先保持全量，并在 UI 中明确说明原因
- 新重要记忆：当前“增量同步”的落地语义已经固定为“轻量版本锚点预检查 + 未变化直接跳过”；`PRTS item/operator/stage/recipe` 用 `revision`，`Penguin` 用 `HEAD Last-Modified` 三元锚点（`matrix|stages|items`）；`官方公告` 与当前 `PRTS growth` 仍走全量，但也会把“请求模式 / 实际执行模式 / 结果状态”明确回传给 CLI 与 desktop
- 已完成：在 `akbox-data::sync` 中新增统一 `SyncMode` / `SyncRunStatus`，并为各同步 outcome 补充 `requested_mode / effective_mode / run_status`；新增 `sync_*_with_mode` 入口，并保留旧的全量兼容函数；`PRTS item/operator/stage/recipe` 已接入轻量 revision 预检查，命中相同 revision 且本地已有缓存与数据时会直接跳过；`Penguin` 已接入 `HEAD` 预检查 `Last-Modified`，首次增量会写入三元锚点，后续同锚点时直接跳过完整拉取；`官方公告` 与当前 `PRTS growth` 在请求增量时会明确回报“实际执行为全量”；CLI `sync` 子命令现在统一支持可选 `--full`，默认增量；desktop 同步页新增“全部同步”开关，默认关闭为增量，并补充各源的增量能力说明与模式/结果提示；补充单测覆盖 `PRTS item` 的 revision 跳过、`Penguin` 的 `Last-Modified` 跳过，以及 CLI `--full` 参数解析；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `sync prts-items --full` 后再 `sync prts-items`，确认第二次返回“增量 / 未变化，已跳过”，当前实际写库 `889` 条 item 定义；已用真实网络执行 `sync penguin --full` 后连续两次 `sync penguin`，确认第一次增量用于写入锚点、第二次返回“增量 / 未变化，已跳过”，当前矩阵 `7791` 条；另已执行 `cargo build -p akbox-desktop --release` 生成新的 desktop release 产物
- 未完成：`PRTS 基建技能 -> external_operator_building_skill` 仍未开始；`官方公告` 仍未过滤创作征集、制作组通讯等非活动内容；当前 `PRTS growth` 仍缺稳定轻量增量锚点
- 风险/阻塞：`PRTS growth` 目前仍依赖逐干员 section 页面，若后续要补增量，需要继续设计“单页变更发现”能力；`Penguin` 的全量后第一次增量会先补写 `Last-Modified` 锚点，因此通常要到第二次增量才会稳定命中跳过，这是当前实现的已知行为
- 下一步：继续停留在 M3，回到既定顺序进入 `PRTS 基建技能 -> external_operator_building_skill`；若后续继续打磨同步体验，优先处理 `PRTS growth` 的安全增量发现机制，再考虑官方公告是否出现可用的版本锚点
