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
- PRTS 的首个同步入口当前走 MediaWiki API `https://prts.wiki/api.php?action=query&meta=siteinfo&siprop=general&format=json`；`sync prts` 会把原始响应写入 `raw_source_cache`，并更新 `sync_source_state` 的成功/失败状态。
- M3 后续除了 CLI 入口外，还要把 PRTS 与 Penguin 的同步结果直接暴露到 GUI 标签页；当前轮次展示内容先以“状态 + 缓存摘要 + 若干结果行”为主，先保证能看、能验证，再根据用户反馈收缩。
- desktop 现在已有独立“同步”页，并提供 `PRTS` / `Penguin` 两个标签；同步动作通过后台线程执行，避免直接阻塞 GUI 事件循环。
- Penguin 当前同步入口固定为 `https://penguin-stats.io/PenguinStats/api/v2/result/matrix?server=CN`；成功时会写入 `raw_source_cache(cache_key = penguin:matrix:cn)`、更新 `sync_source_state(source_id = penguin.matrix.cn)`，并刷新 `external_drop_matrix`。

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
11. 打通外部数据同步骨架（进行中：PRTS 与 Penguin 已完成且已接入 GUI；下一步转入官方公告客户端）
12. 建立 AGENTS.md 更新习惯（进行中，已完成多次记录）
13. 将 desktop 的“导出调试样例”改为真实截图导出入口，为 M4 的 ADB 截图接入预留 UI 和接口，但不提前实现真实抓图（已完成）
14. 为 PRTS 与 Penguin 增加 GUI 标签页展示当前同步内容与结果摘要（已完成）

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
