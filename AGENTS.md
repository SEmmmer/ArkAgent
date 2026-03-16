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
  - 协议逆向后的私有接口（原约束；已被 2026-03-17 用户新决策部分替代：对于《明日方舟》国服，森空岛接口现视为可作为主采集方案的官方接口，不再按“私有逆向接口”处理；其余未明确确认的协议逆向接口仍禁止作为主方案）
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

- 森空岛：账号绑定、干员拥有状态、干员养成状态、模组/专精/基建状态、box 与基建当前态；当前已提升为“玩家拥有状态”的第一优先数据源
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
2. 森空岛官方接口
3. 高置信度本地识别
4. 中置信度识别 + 人工确认
5. 历史已确认状态
6. LLM 辅助推断（不得直接落最终态）

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
- 关于官方公告，上一轮“`notice_type = activity` + `start_at` + 黑名单关键词”的规则过滤已被新要求替代：用户现在要求这里只收“会开放资源关卡”的活动（如故事集 / SideStory），而不是泛 `ACTIVITY` 或泛活动公告。当前仅靠官网索引页标题/摘要和规则匹配，无法稳定准确区分“会开资源关卡的活动”和“联动活动 / 限定寻访 / 特殊玩法活动”；因此现阶段 `sync official` 继续缓存官网全量原始页，并把 `external_event_notice` 也保留为官网全量公告当前态，不在规则层硬做这类语义筛选，后续留给更强分类器或 DeepSeek。
- PRTS 的首个结构化业务数据入口当前落在 MediaWiki API `action=parse&page=道具一览&prop=revid|text&format=json`；同步入口为 `akbox-cli sync prts-items [database_path]`，成功时会写入 `raw_source_cache(cache_key = prts:item-index:cn)`、更新 `sync_source_state(source_id = prts.item-index.cn)`，并 upsert 到 `external_item_def`；Penguin 的 item stub 在主键冲突时只保留占位插入，不再覆盖 PRTS 已同步的正式道具定义。
- PRTS 的关卡静态映射当前通过两步 MediaWiki API 组合获取：先用 `action=parse&page=关卡一览&prop=revid&format=json` 取 revision，再用 `action=ask&query=[[关卡id::+]]|?关卡id|?分类|limit=500[|offset=n]&format=json` 分页拉取结构化关卡索引；同步入口为 `akbox-cli sync prts-stages [database_path]`，成功时会写入 `raw_source_cache(cache_key = prts:stage-index:cn)`、更新 `sync_source_state(source_id = prts.stage-index.cn)`，并把 PRTS 负载挂到 `external_stage_def.raw_json.$.prts`，避免覆盖 Penguin 的 stage 根对象。
- PRTS 配方当前通过 MediaWiki API `action=parse&page=罗德岛基建/加工站&prop=revid|text&format=json` 获取，再解析加工站配方表落到 `external_recipe`；同步入口为 `akbox-cli sync prts-recipes [database_path]`，成功时会写入 `raw_source_cache(cache_key = prts:recipe-index:cn)`、更新 `sync_source_state(source_id = prts.recipe-index.cn)`；由于实时页面里存在相同产物/等级的重复配方行，当前 `recipe_id` 采用 `workshop:{output_item_id}:lv{level}:row{n}`，并在每次同步时全量替换 `external_recipe`。
- PRTS 配方里的道具名解析当前不能简单按“同名即报错”处理；与 Penguin 共库时会出现同名旧 id / 别名 item（如 `碳` 同时命中 `200008` 与 `3112`）。当前规则是：同名时优先选择带 PRTS 正式负载的定义；若都只来自 Penguin，再优先选择 `item_id == sortId` 的 canonical 项；只有仍然无法判定时才报真正的歧义错误。
- PRTS 干员定义里，`分类:专属干员` 当前可作为“模式 / 活动专属、玩家常规 box 不可拥有干员”的稳定标记；`sync prts-operators` / `sync prts` 现会在写入 `external_operator_def` 前过滤这类干员，并采用替换写入而不是纯 upsert，确保旧库里残留的 `Mechanist(卫戍协议)`、`暮落(集成战略)`、预备干员等条目会在下次同步时被清掉。
- PRTS 基建技能当前来自单干员页 `后勤技能` section；同步入口为 CLI `akbox-cli sync prts-building-skills [--full] [database_path]` 与 desktop“同步 PRTS 基建技能”，成功时会写入 `raw_source_cache(cache_key = prts:operator-building-skill:cn)`、更新 `sync_source_state(source_id = prts.operator-building-skill.cn)`，并全量替换 `external_operator_building_skill`；`room_type` 列当前存 canonical key（如 `trading_post` / `control_center`），中文房间名与描述保留在 `raw_json` 里供展示。
- 对 `PRTS growth / PRTS building skill` 这类逐干员 section 同步，若目标是“严格不漏任何变化”，当前不能接受基于页级 `lastrevid / touched` 的 workaround 式增量发现；在拿不到可证明覆盖完整数据单元的全局稳定锚点前，这两条链路继续保持全量同步，不为了省请求量引入后续难以收敛的伪增量语义。
- 当前用户已确认正式进入 M4；本轮目标不再停留在设备页占位，而是持续推进到“MuMu 启动后可以稳定抓到真实游戏截图”为止。M4 的最小闭环顺序固定为：ADB 可执行文件发现 -> MuMu 端口发现 -> `DeviceSession` 连接/重连 -> `exec-out screencap -p` 真截图链路 -> desktop 设备页实时预览；在真截图未打通前，不提前扩张更高层自动化动作。
- `akbox-device` 当前已经落地真实 M4 基础链路：`DeviceSession`、ADB 可执行文件自动发现、MuMu 默认端口探测（`127.0.0.1:7555` 与 `16384 + 32 * n`）、手动串号/端口覆盖、`adb connect`、`exec-out screencap -p` 真截图、以及对 loopback 设备截图失败时的一次重连重试；desktop 也已有独立“设备”页，连接检查与截图抓取都走后台线程并显示真实 PNG 预览。
- 当前这台机器上的 MuMu / 方舟实例实机验证已经完成：自动发现命中的实际 ADB 是 `C:\Program Files\YXArkNights-12.0\shell\adb.exe`，实际设备串号是 `127.0.0.1:7555`；`akbox-cli debug capture-device` 已连续三次成功抓到 `1920x1080` PNG，三次 SHA256 一致，采样颜色数为 `71`，可确认当前抓到的是稳定真实游戏画面而不是空帧/纯色帧。
- desktop 里的 PRTS 同步入口后续不再长期维持“站点 / 道具 / 关卡”多个分散按钮；随着 PRTS 结构化同步项增加，应收敛成一个“同步 PRTS 全部”按钮，在后台顺序执行当前所有 PRTS 子同步并统一回填概览。
- desktop 同步页后续必须按“玩家可读”而不是“源数据直出”展示：不再默认展示 `source_id` / `cache_key` / `content_type`；时间统一转换到用户配置的时区；Penguin 需要把 `main_01-07` 之类的 stage id 转成玩家可读名称、把 item id 转成游戏内道具名、按关卡聚合并按掉率降序展示材料，同时展示单材料期望体力；当前掉落预览还需要按关卡热度（最近一段时间的上传数量）排序，并优先展示“正在进行中的活动里且掉落蓝色材料”的关卡，其余部分再展示“当前可访问的全部关卡”；掉落展示上要区分常规掉落与特殊掉落，`EXTRA_DROP` / 额外物资默认折叠且不展示；官方公告后续若要单独展示“会开放资源关卡的活动”，不能只靠索引页简单规则硬筛，至少需要更强的全文规则分类或 DeepSeek 辅助，当前阶段先保留官网全量公告。
- desktop 的长页面需要默认具备滚动能力，不能因为内容变长导致底部信息被截断；本轮同步页收口时一并补上页面滚动容器。
- 2026-03-17 起，项目约束已由用户明确修改：森空岛接口现视为《明日方舟》国服的官方高精度数据源，优先级高于 OCR；后续干员拥有/养成状态同步应优先考虑森空岛导入，再用 MuMu + ADB + 本地视觉识别做校验、补洞和无接口兜底，而不再坚持 OCR 为第一主入口。

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
11. 打通外部数据同步骨架（已完成：PRTS 站点 / PRTS 干员基础资料 / PRTS 道具索引 / PRTS 关卡静态映射 / PRTS 配方 / PRTS 养成需求 / PRTS 基建技能 / Penguin / 官方公告 已完成且已接入 GUI；干员定义现已按 `分类:专属干员` 过滤 box 不可拥有的临时干员；同步页首轮玩家可读收口、长页面滚动、Penguin 预览排序 / 掉落分组 / 当前可访问判定已完成；官方公告现阶段保留官网全量公告当前态，“只保留会开放资源关卡的活动”暂不在规则层硬筛，留待更强分类器或 DeepSeek；desktop 与 CLI 的 PRTS 入口已收敛为“常规同步 + 独立 growth / building skill”；`PRTS growth / building skill` 在缺少可证明不漏变化的全局锚点前继续保持全量，不做 workaround 式增量发现）
12. 完成 M4 / MuMu-ADB 接入（已完成：自动发现设备、手动指定串号/端口、连接/重连、`exec-out screencap -p` 真截图、`tap/swipe/keyevent` 输入链路、desktop 设备页实时截图预览；当前设备运行态输入仍未持久化到配置文件，但不阻塞阶段完成）
13. 建立 AGENTS.md 更新习惯（进行中，已完成多次记录）
14. 将 desktop 的“导出调试样例”改为真实截图导出入口，为 M4 的 ADB 截图接入预留 UI 和接口，但不提前实现真实抓图（已完成）
15. 为 PRTS 与 Penguin 增加 GUI 标签页展示当前同步内容与结果摘要（已完成）
16. 为官方公告增加 GUI 标签页展示同步状态与公告摘要（已完成）
17. 将森空岛 `player/info` 收敛成只读 CLI / desktop 调试入口，先稳定输出脱敏字段摘要与 JSON shape，再规划导入 `operator_snapshot / operator_state`（新增，当前优先级高于 OCR 干员扫描）
17. 进入 M5 / 视觉基础设施（进行中：页面状态 / ROI 配置结构、PNG 局部裁剪、`scan_artifact` / `recognition_review_queue` 最小写入闭环、模板匹配骨架、OCR 封装类型、CLI `debug vision-inspect` 最小视觉调试入口均已完成；下一步优先把真实 OCR 后端或可替代识别后端接进这条调试链，再继续模板匹配/扫描入口收口）

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

- 日期时间：2026-03-16 20:26:33 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：将当前这一轮 M3 的累计改动整理成多个清晰 commit，并推送到远端分支
- 新重要记忆：本轮历史整理已固定为 3 个主题 commit 加 1 个后补文档 commit：`feat(data): add external sync pipelines`、`feat(desktop): add sync controls and player previews`、`docs: update AGENTS milestone log`，以及当前这条“交付完成”记录；后续若继续推进 M3，应在这个提交基线上增量开发，不再回到单个大工作区长期堆积
- 已完成：已将当前累计改动拆分并提交到 `master`：`d24b755 feat(data): add external sync pipelines` 负责 `akbox-data` 的官方公告 / PRTS / Penguin 同步能力、repository 与 sync 核心；`51088b7 feat(desktop): add sync controls and player previews` 负责 CLI / desktop 的同步入口、增量开关与玩家视角展示；`f6d7ad2 docs: update AGENTS milestone log` 负责前序里程碑记录整理；随后已成功执行 `git push origin master`，远端 `origin/master` 已更新到这些提交
- 未完成：`external_operator_building_skill` 仍未开始；官方公告仍未过滤创作征集、制作组通讯等非活动内容；当前 `PRTS growth` 仍缺稳定轻量增量锚点
- 风险/阻塞：虽然提交已拆分，但第一批数据层提交包含了较大范围的 M3 累计改动；若后续还要进一步细化历史，只能通过后续追加整理，不应再改写已推送到远端的公开历史
- 下一步：继续停留在 M3，按既定顺序进入 `PRTS 基建技能 -> external_operator_building_skill`；同时保持“每完成一个最小闭环就及时提交”，避免再次形成跨多主题的大工作区积压

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

- 日期时间：2026-03-16 22:09:30 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户已确认正式进入 M5；首个闭环不直接跳 OCR 或模板匹配，而是先建立可落库、可复用的 ROI 配置机制最小骨架，并把识别前产物与低置信度复核入口接到数据库
- 新重要记忆：M5 的第一步语义已固定为“ROI 定义 + 页面级状态骨架 + 局部截图裁剪 + `scan_artifact` / `recognition_review_queue` 最小写入闭环”；在这一步之前，不把零散坐标硬编码扩散进扫描流程，也不急着引入真正 OCR 引擎
- 已完成：已先确认当前仓库里 `scan_artifact`、`recognition_review_queue` 表已经由 migration 提供，但 repository 与 device 侧尚无对应写入和 ROI 抽象；M5 将从这两个缺口补起
- 未完成：代码尚未开始；ROI 配置格式、页面状态机骨架、截图裁剪 API、artifact/review_queue 写库接口与验证都还没有落地
- 风险/阻塞：如果这一步直接把 ROI 坐标、页面判断和识别策略写死在具体扫描逻辑里，后续模板匹配 / OCR / review 流程会很快失控；因此必须先把结构层搭出来，再继续识别实现
- 下一步：先在 `akbox-device` 定义页面模板与 ROI 结构、截图裁剪结果与基础验证，再在 `akbox-data::repository` 增加 `scan_artifact` / `recognition_review_queue` 写入接口，最后补最小 CLI 或单测验证并回写本文件

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

### 变更记录

- 日期时间：2026-03-16 20:45:04 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：按既定里程碑继续完成 `PRTS 基建技能 -> external_operator_building_skill`，并保持“常规 PRTS 同步”和 section 型重同步入口分离，不把高耗时、缺少稳定增量锚点的单干员 section 再塞回 `sync prts`
- 新重要记忆：PRTS 的基建技能数据源当前稳定落在单干员页 `后勤技能` section；由于它和 `PRTS growth` 一样来自逐干员 section 拉取，当前增量请求会明确回退为全量；写库时 `external_operator_building_skill.room_type` 当前保存 canonical key（如 `trading_post` / `control_center` / `dormitory`），中文房间名、解锁条件、描述、图标信息保留在 `raw_json` 中供 GUI 直接展示
- 已完成：在 `crates/akbox-data::prts` 中新增 `fetch_operator_building_skills`、`PrtsOperatorBuildingSkillResponse`、`PrtsOperatorBuildingSkillDefinition` 与 `后勤技能` section 解析，支持从多张表中提取解锁条件 / 房间 / 技能名 / 描述 / 图标；在 repository 中新增 `external_operator_building_skill` 的 replace / count / list 与玩家可读解析记录，并沿用干员替换时清理 stale building skill 的策略；在 sync 层新增 `PRTS_OPERATOR_BUILDING_SKILL_SOURCE_ID = prts.operator-building-skill.cn`、`PRTS_OPERATOR_BUILDING_SKILL_CACHE_KEY = prts:operator-building-skill:cn`、独立 outcome / error / sync 函数，写入 `raw_source_cache`、`sync_source_state` 与 `external_operator_building_skill`；CLI 新增 `sync prts-building-skills [--full] [database_path]`；desktop 同步页新增“同步 PRTS 基建技能”按钮、后台任务分支、状态提示与 PRTS 概览里的基建技能预览；补充 client / repository / sync / CLI 测试，覆盖 section 解析、写库、失败告警与参数校验；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -q -p akbox-cli -- sync prts-building-skills <temp_db>`，确认当前实时 revision 为 `385847`，写入 `873` 条基建技能记录
- 未完成：官方公告仍未过滤创作征集、制作组通讯等非活动内容；`PRTS growth` 与 `PRTS building skill` 仍缺稳定轻量增量锚点；desktop 仍未给 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：当前基建技能解析依赖 PRTS `后勤技能` section 的表头仍保持“条件 / 图标 / 技能N / 房间 / 描述”结构；若后续模板改版，需要同步更新解析器；此外当前 `skill_id` 仍带顺序型 `rowN` 后缀，只适合“每次全量替换”语义，若未来改成真正增量 upsert，需要再设计更稳定的行级主键
- 下一步：继续停留在 M3，优先处理官方公告“仅展示真正活动公告”的过滤闭环；若继续打磨同步层，再评估 `PRTS growth / building skill` 的安全增量发现机制，而不是直接跳到 M4

### 变更记录

- 日期时间：2026-03-16 21:26:29 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：继续按既定里程碑收口官方公告同步，只把“真正可用于活动窗口与提醒”的活动公告写入 `external_event_notice`，不再把创作征集、制作组通讯、维护/更新公告这类噪音继续暴露给规划与提醒层
- 新重要记忆：实时官方官网的 `ACTIVITY` 分栏并不等于“纯活动公告”集合，当前已经确认会混入 `更新公告` 与 `创作征集活动`；因此本地过滤规则固定为“`notice_type = activity` + 能解析 `start_at` + 关键词黑名单”，并且 `external_event_notice` 在每次官方同步时采用全量替换，避免旧库残留的非活动公告继续污染当前态
- 已完成：在 repository 中新增 `replace_external_event_notices`，把官方公告写库从增量 upsert 改为全量替换；在 `sync_official_notices_with_mode` 中加入活动公告过滤逻辑，仅保留满足规则的 `activity` 公告写入 `external_event_notice`，同时继续缓存官网原始全量页面到 `raw_source_cache`；desktop 同步页与官方公告概览文案已改成“活动公告”口径，明确说明原始缓存保留全量、当前表只留真实活动；CLI `sync official` 输出已改为 `Activity notice count`；同步层测试已覆盖“旧非活动公告会被清掉、更新公告/创作征集/制作组通讯不会写入活动表”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -q -p akbox-cli -- sync official <temp_db>`，当前 revision 为 `2026-03-11T11:00:00+08:00`，实际写入 `10` 条活动公告
- 未完成：官方公告过滤当前仍是规则型启发式，不是更细粒度的结构化分类器；`PRTS growth` 与 `PRTS building skill` 仍缺稳定轻量增量锚点；desktop 仍未给 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：当前活动过滤依赖标题/摘要关键词和 `start_at` 解析结果；若官网文案风格变化、或后续出现“真实活动但标题命中黑名单”的边界样本，需要继续调整规则；另外 `external_event_notice` 现按当前态全量替换，更适合提醒/规划读取，不等价于完整公告历史档案
- 下一步：继续停留在 M3，回到 PRTS 主线，优先评估 `PRTS growth / building skill` 的安全增量发现机制，先做 section 型同步的轻量变更发现，再决定是否继续扩展同步体验

### 变更记录

- 日期时间：2026-03-16 21:34:10 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：官方公告页里若要单独保留“活动公告”，语义必须进一步收紧为“会开放资源关卡的活动”，例如故事集 / SideStory；联动活动但不开关卡、限定寻访、制作组通讯、维护/更新公告等都不应混进这个集合。若在未接入大模型前无法稳定准确判断，则这里暂时不做语义硬筛
- 新重要记忆：当前仅靠官网索引页的 `title / brief / notice_type / start_at` 无法稳定准确判定“是否属于会开放资源关卡的活动”。即使继续补全文规则，也至少需要抓取公告详情页正文，再结合“关卡开放时间 / 关卡编号 / 解锁条件 / 活动道具”等信号与跨源校验；在现阶段没有这套更强分类链路前，`external_event_notice` 应保留官网全量公告当前态，不把不可靠的规则筛选伪装成准确语义
- 已完成：已把上一轮“活动公告硬筛”从官方同步链路中回退：`sync_official_notices_with_mode` 现恢复写入官网全量公告到 `external_event_notice`，不再按 `activity + start_at + 黑名单关键词` 过滤；desktop 与 CLI 文案已从“活动公告记录”改回“官方公告记录”，并明确说明“会开放资源关卡的活动”语义暂不在规则层硬筛；同步层测试已同步回退为验证“旧公告会被替换掉，但官网全量公告会完整写入”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已用真实网络执行 `cargo run -q -p akbox-cli -- sync official <temp_db>`，当前 revision 为 `2026-03-14T11:30:00+08:00`，实际写入 `24` 条官方公告
- 未完成：官方公告详情页正文尚未接入；“会开放资源关卡的活动”仍没有稳定的本地全文规则分类器；`PRTS growth` 与 `PRTS building skill` 仍缺稳定轻量增量锚点；desktop 仍未给 PRTS 全量同步提供更细粒度的子步骤进度回传
- 风险/阻塞：如果后续要在不依赖 DeepSeek 的前提下做这类分类，必须把官方同步从“列表页索引”扩展到“列表页 + 公告详情页正文 + 明确规则集”，工作量和误判面都明显高于当前版本；在这之前，任何基于索引标题/摘要的硬筛都只能是近似，不适合承诺“这里只会有资源关卡活动”
- 下一步：继续停留在 M3，先回到 PRTS 主线，优先评估 `PRTS growth / building skill` 的安全增量发现机制；官方公告这条线先维持全量同步，待后续再决定是否接入正文级规则分类或直接交给 DeepSeek

### 变更记录

- 日期时间：2026-03-16 21:39:20 +08:00
- 阶段：M3 / 阶段 3：外部数据同步骨架
- 新需求：`PRTS growth / PRTS building skill` 的同步若目标是“严格不漏任何变化”，则不能接受页级 `lastrevid / touched` 之类的 workaround 式增量发现；宁可继续全量，也不要先引入会让后续实现和语义收敛变困难的伪增量
- 新重要记忆：当前 live PRTS 的 MediaWiki API 虽然能拿到单页 `lastrevid / touched`，但这只能证明“有可用的页级变更信号”，不能证明“足以覆盖 section 最终渲染的全部变化来源”；因此在本项目里，这类逐干员 section 数据源仍然不应被视为具备严格安全的增量锚点
- 已完成：已确认并冻结这一实现边界：不再继续推进 `PRTS growth / building skill` 的页级增量发现方案，相关后续计划从“评估安全增量发现”改为“维持全量同步，等待真正可证明不漏变化的全局锚点或更高可信的数据源”
- 未完成：`PRTS growth / building skill` 仍然只有全量同步，没有严格安全的增量能力；官方公告“资源关卡活动”语义分类也仍未接入正文级规则或 DeepSeek
- 风险/阻塞：如果未来仍坚持“严格不漏变化”，那么 section 型同步的请求量和耗时会继续偏高；除非后续找到真正的全局稳定锚点，否则这部分没有低成本又严格正确的增量路线
- 下一步：继续停留在 M3，但不再为 `PRTS growth / building skill` 设计 workaround 增量；优先回到其他未完成同步骨架收口项，或等待更高可信的数据源/锚点出现后再重开这条线

### 变更记录

- 日期时间：2026-03-16 21:53:50 +08:00
- 阶段：M4 / 阶段 4：MuMu / ADB 接入
- 新需求：正式进入 M4，并持续推进到“MuMu 启动后可以稳定抓到真实游戏截图”为止；在真截图稳定前，不继续扩张更高层自动化动作
- 新重要记忆：M4 的最小闭环现已固定为“ADB 可执行文件发现 -> MuMu 端口发现 -> `DeviceSession` 连接/重连 -> `exec-out screencap -p` 真截图 -> desktop 设备页实时预览”；当前实现里若截图对象是 loopback MuMu 端点，首次 `screencap` 失败或返回非 PNG 时会先做一次 `adb disconnect/connect` 后重试
- 已完成：在 `akbox-device` 中把原占位 `BackendNotReady` 替换为真实设备链路，新增 `DeviceConnectRequest` / `DeviceSession` / `DeviceConnectionInfo` / `ScreenshotCaptureResult`，支持显式 `adb.executable`、PATH 查找、常见 MuMu 安装目录查找、MuMu 默认端口探测、手动串号/端口覆盖、`adb devices` 解析、`adb connect`、`exec-out screencap -p` 抓取和 PNG 校验；新增单测覆盖端口候选生成、`adb devices` 解析、已连接设备直连、自动 connect 候选端口、以及 loopback 截图失败后的重连重试；desktop 已新增独立“设备”页，支持手动串号/端口输入、自动实例探测数输入、后台“刷新设备连接”与“抓取截图预览”任务、真实 PNG 解码与预览；设置页里的“导出真实截图”现也复用同一条真实设备抓图链路；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：尚未做实机 MuMu 截图验证；`tap / swipe / keyevent` 仍未接；设备页当前的“手动串号 / 端口”和“自动探测实例数”还是运行态输入，还没持久化进配置文件
- 风险/阻塞：当前 ADB 自动发现虽然已经补了 PATH 与常见 MuMu 安装目录查找，但真实用户机器上的安装目录命名可能继续分叉；若当前自动发现仍 miss，需要先用设置页显式填写 `adb.executable`；另外，真机稳定性还必须经过实际 MuMu 运行态验证，单元测试只能证明链路逻辑正确
- 下一步：在用户已启动 MuMu 的前提下，立即做实机连接与截图验证；若命中自动发现或连接问题，先用同机实际安装路径和端口修正，再一直迭代到能稳定抓到游戏画面为止

### 变更记录

- 日期时间：2026-03-16 21:56:27 +08:00
- 阶段：M4 / 阶段 4：MuMu / ADB 接入
- 新需求：在 M4 中不仅要把代码链路接上，还要在当前同机 MuMu 实例上完成真实截图验证，一直做到能稳定抓到游戏画面为止
- 新重要记忆：当前机器上的 MuMu / 方舟实例实际通过 `127.0.0.1:7555` 暴露 ADB；自动发现命中的实际 `adb.exe` 路径是 `C:\Program Files\YXArkNights-12.0\shell\adb.exe`。后续如果用户机器上再次出现“留空 adb 路径仍能工作”的情况，优先检查这一路径是否仍然可用；若换机或重装后自动发现失效，再回退到设置页显式填写
- 已完成：已新增 CLI 调试入口 `akbox-cli debug capture-device [--config path] [--serial serial_or_port] [output_path]`，复用同一条 `akbox-device` 真实链路做无 GUI 抓图验证；已在当前机器上用 `cargo run -q -p akbox-cli -- debug capture-device` 成功自动发现 `127.0.0.1:7555` 并抓到真实截图，输出文件为 `debug-artifacts/cli-device-capture.png`，PNG 大小 `2000761` 字节；随后又用 `--serial 7555` 连续抓取 `cli-device-capture-2.png` 与 `cli-device-capture-3.png`，三次都成功，尺寸均为 `1920x1080`，SHA256 一致；另做了采样颜色检查，`sampled_unique_colors = 71`，可确认不是纯黑/纯色空帧；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：`tap / swipe / keyevent` 仍未接；desktop 设备页虽然已接到同一条真实抓图链路，但本轮主要通过 CLI 做了实机验证，还未额外做 GUI 人工点击验收；设备相关运行态输入还没持久化进配置文件
- 风险/阻塞：当前自动发现依赖 `adb.exe` 可在 PATH 或常见 MuMu 安装目录中被发现；如果后续 MuMu 安装目录命名再次变化，可能需要补新的目录规则或回退到手动填写 `adb.executable`；另外，虽然连续抓图已经稳定，但后续若接 `tap/swipe/keyevent`，还要继续验证连接保持与截图刷新之间不会互相干扰
- 下一步：继续停留在 M4，在真截图链路稳定的前提下进入输入动作最小闭环，优先实现 `tap / swipe / keyevent` 与 desktop 设备页的连接状态刷新；若用户先要更顺手的使用路径，也可以先把当前运行态串号/端口输入收进配置文件

### 变更记录

- 日期时间：2026-03-16 22:07:12 +08:00
- 阶段：M4 / 阶段 4：MuMu / ADB 接入
- 新需求：用户已确认真实截图没有问题，继续按既定里程碑把 M4 从“真截图稳定”推进到“真实输入动作链路可用”，优先补齐 `tap / swipe / keyevent`，并在不干扰当前游戏画面的前提下完成至少一条低风险实机验证
- 新重要记忆：当前 `akbox-device` 已在同一条真实设备链路上支持 `tap / swipe / keyevent`；desktop 设备页现已提供运行态输入控件，CLI 也新增 `akbox-cli debug keyevent [--config path] [--serial serial_or_port] key_code` 入口。实机低风险验证当前采用 `keyevent 0`，以确认输入通道可达且不会强制跳出当前游戏页面
- 已完成：在 `crates/akbox-device` 中新增 `DeviceInputAction` / `DeviceInputRequest` / `DeviceInputResult` / `DeviceInputError` 与 `send_device_input`，通过 `adb shell input tap|swipe|keyevent` 复用现有 `DeviceSession`、设备发现和 loopback 失败后重连重试能力；desktop 设备页已新增点按 / 滑动 / 按键三类运行态输入控件、后台任务分支与结果提示；CLI 已新增 `debug keyevent` 调试入口与参数校验测试；补充单测覆盖已连接设备按键、loopback 点按失败后重连重试、非 loopback 滑动失败直返错误；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；已在当前机器上执行 `cargo run -q -p akbox-cli -- debug keyevent --serial 7555 0`，成功通过 `C:\Program Files\YXArkNights-12.0\shell\adb.exe` 向 `127.0.0.1:7555` 发送输入；随后执行 `cargo run -q -p akbox-cli -- debug capture-device --serial 7555 debug-artifacts/cli-device-after-keyevent.png`，成功再次抓取 `1920x1080` PNG，文件大小 `1463491` 字节，SHA256 为 `A1665E5EE3AB225B35A967148AA4C67DE8A9EE7A7F8680612A483ACABAC067F0`
- 未完成：尚未对真实游戏画面直接执行 `tap` / `swipe` 实机操作，以避免在用户当前账号界面上产生副作用；desktop 设备页的串号/端口/动作输入仍是运行态表单，尚未持久化进配置文件；M5 的 ROI / 页面状态机 / OCR 仍未开始
- 风险/阻塞：`tap` / `swipe` 的真实验证天然带有副作用，后续若要继续做实机验收，必须先约束测试坐标或由用户确认可操作页面；另外，当前 ADB 自动发现仍依赖 PATH 或常见 MuMu 安装目录命中 `adb.exe`，若用户换机或重装 MuMu，可能仍需回退到手动填写 `adb.executable`
- 下一步：正式结束 M4，进入 M5 / 视觉基础设施；先建立 ROI 配置机制、页面状态机骨架和 `scan_artifact` / `recognition_review_queue` 的最小写入闭环，再继续往模板匹配与 OCR 封装推进

### 变更记录

- 日期时间：2026-03-16 22:44:30 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户明确要求把当前 CLI 的 M5 视觉调试能力链接进 GUI；要求在 desktop 中提供一个最小可用的视觉调试入口，能够加载页面配置、输入页面 id 和本地 PNG、运行页面确认/ROI 裁剪，并展示结果摘要
- 新重要记忆：这一步应复用现有 `akbox-device` 视觉骨架和 `vision-inspect` 语义，而不是在 desktop 里另起一套不兼容的逻辑；GUI 先做“本地文件输入 + 后台任务 + 结果摘要”，不在这一轮扩成完整扫描工作台
- 已完成：已先确认 `apps/akbox-desktop/src/main.rs` 里当前只有设置 / 设备 / 同步三块状态，没有视觉调试入口；准备以新增独立页面状态的方式接入，避免继续把 M5 调试能力塞进设备页
- 未完成：代码尚未开始；视觉调试页、后台任务、结果展示和与 `vision-inspect` 对应的 GUI 输入字段都还没有落地
- 风险/阻塞：如果把视觉调试硬塞进设备页，会继续混淆 M4 的设备调试和 M5 的视觉调试；更稳的是新增独立页面或至少独立段落，保持后续扫描页扩展空间
- 下一步：检查 desktop 导航和后台任务模型，新增最小“视觉调试”页并复用 `akbox-device` 视觉链路；完成后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-16 22:40:40 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：在模板匹配骨架和 `vision-inspect` 调试入口代码完成后，再补一条脱离单元测试的命令级验收，确认 CLI 在临时页面配置 / 模板 / 样例 PNG 上能独立跑通
- 新重要记忆：`akbox-cli debug vision-inspect` 当前不仅有单测，还已经做过一次真实 CLI 命令验收；后续若用户反馈视觉配置问题，可以先要求提供页面配置 JSON、模板 PNG 和截图，再直接复用这条命令定位，而不必先接入正式扫描流程
- 已完成：已在当前机器上用临时生成的 `assets/templates/pages/inventory_main.json`、`markers/inventory_main/title_marker.png` 与本地样例截图执行 `cargo run -q -p akbox-cli -- debug vision-inspect <page_config> inventory_main <input_png> <output_dir>`，命令返回成功；实际输出显示 `Page matched: yes`、`Marker match count: 1/1`、`ROI output count: 1`，并成功写出 `manifest.json`
- 未完成：这次命令级验收仍基于合成样例，不是实际《明日方舟》页面截图；真实 OCR 后端也还没接入，因此 manifest 里的 OCR 部分仍会是结构化错误/跳过信息
- 风险/阻塞：如果后续真实游戏页面模板出现缩放、抗锯齿或 UI 微变体，仅靠当前灰度差分骨架可能会有误判；正式接入真实页面前，仍需要逐步收集真实样本并评估阈值
- 下一步：继续停留在 M5，优先接入真实 OCR 后端或本地替代识别后端，再用真实页面截图扩展 `vision-inspect` 和模板匹配回归样本

### 变更记录

- 日期时间：2026-03-16 22:39:33 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：设备页布局修补验收通过后，继续按 M5 主线推进模板匹配骨架、OCR 封装类型和最小视觉调试入口；要求在正式扫描状态机落地前，就能对本地 PNG、页面配置和模板文件做独立验证
- 新重要记忆：当前视觉调试链路已经固定出一个最小入口：`akbox-cli debug vision-inspect [--templates-root path] <page_config_path> <page_id> <input_png> [output_dir]`。模板根目录默认按 `assets/templates/pages/<page_id>.json` 推导到 `assets/templates`；页面确认 marker 当前优先支持 `template_fingerprint / icon_template`，匹配算法骨架为 `normalized_grayscale_mae`；OCR 当前只完成统一封装和结构化错误返回，仍未接入真实后端
- 已完成：在 `akbox-device` 中新增 `ocr.rs`，落地 `OcrRequest` / `OcrResult` / `OcrError` / `OcrBackend` 与 `recognize_text_from_png` 统一入口；在 `vision.rs` 中为 `PageConfirmationMarker` 增加 `match_method / template_path / pass_threshold`，新增 `TemplateMatchMethod`、`MarkerMatchResult`、`PageConfirmationResult` 与 `evaluate_page_confirmation_from_png`，当前模板匹配骨架会按 ROI 区域和模板 PNG 的灰度平均绝对误差计算 0..1 相似度；在 `apps/akbox-cli` 中新增 `debug vision-inspect` 命令，支持加载页面配置、推导模板根目录、执行页面确认、裁剪全部 ROI、对 OCR 类 ROI 调用统一 OCR 封装并把结果/错误写入 `manifest.json`；同时新增 [README.md](C:/Users/emmmer.SUPERXLB/git/ArkAgent/assets/templates/README.md) 约束 `assets/templates` 下的页面配置和模板命名规则；补充测试覆盖模板匹配成功、缺失模板路径报错、CLI `vision-inspect` 参数校验、默认模板根目录推导与完整输出产物；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，本轮后 CLI 测试为 `25` 个、device 测试为 `16` 个
- 未完成：OCR 仍然只有封装骨架和结构化“后端不可用”错误，尚未接入真实 Windows OCR 或其他本地识别后端；模板匹配当前只支持最小灰度差分算法，尚未接入更强的图标/哈希/多尺度比对；desktop 还没有直接暴露这条视觉调试入口
- 风险/阻塞：当前 `vision-inspect` 虽然已经能做模板和 ROI 回归，但 OCR 部分会在 manifest 里稳定返回“后端不可用”；因此这一步解决的是“可验证的骨架”和“调试链路”，不是“识别已可用”；另外 `assets/templates` 目录下还没有正式游戏页面模板样本，需要后续逐页补齐
- 下一步：继续停留在 M5，优先接入真实 OCR 后端或至少可运行的本地替代识别后端，再把 `vision-inspect` 的模板/ROI/ocr 结果接入更正式的扫描调试入口；随后再评估 desktop 是否需要直接提供这条视觉调试能力

### 变更记录

- 日期时间：2026-03-16 22:32:51 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户已确认设备页布局修补完成，继续按 M5 主线推进；本轮优先实现模板匹配骨架和 OCR 封装类型，并把已有 `vision` 配置接到一个最小 CLI 视觉调试入口，避免正式扫描前仍缺少可验证通道
- 新重要记忆：M5 这一步的“最小可验闭环”将收敛为“页面配置文件 + 模板匹配骨架 + OCR 抽象类型 + CLI 调试入口”；在没有正式扫描状态机前，也必须能对本地 PNG 和页面配置做独立验证，否则模板/ROI 配置会缺少回归通道
- 已完成：已先把这条继续推进 M5 的需求写回 `AGENTS.md`，准备从 `akbox-device` 的 `vision` 模块和 `apps/akbox-cli` 的 `debug` 子命令切入
- 未完成：代码尚未开始；模板匹配骨架、OCR 封装类型、CLI 视觉调试命令与模板目录规范都还未落地
- 风险/阻塞：如果这一步只加库内抽象、不加可执行调试入口，后续页面模板文件很难被稳定验证；反过来如果先写 CLI 命令但没有统一模板/OCR 抽象，命令又会迅速固化成一次性脚本
- 下一步：先在 `akbox-device` 增加模板匹配骨架和 OCR 封装类型，再给 `akbox-cli debug` 增加最小视觉调试入口，最后补 `assets/templates` 的文件格式/命名规则并执行 fmt / clippy / test

### 变更记录

- 日期时间：2026-03-16 22:29:53 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：desktop 设备页“输入动作测试”区域里，`滑动起点` 这一行当前过长，窗口较窄时会显示不全；需要做最小 UI 收口，保证中文标签和输入框在常规窗口宽度下可读、可操作
- 新重要记忆：这条属于现有 desktop 设备页的人机工学修补，不改变 M4 已完成的真实输入动作链路，也不改变 M5 当前 ROI / artifact 的推进顺序；处理方式应优先改布局而不是改词义或删功能
- 已完成：已先将该 UI 缺陷需求写回 `AGENTS.md`，准备检查 `apps/akbox-desktop/src/main.rs` 中“输入动作测试”区域的布局结构，再做最小补丁
- 未完成：代码尚未开始；是否需要把滑动区从单行改为两行/网格，还未根据当前布局实际结构确认
- 风险/阻塞：若直接缩短标签文案而不调整布局，后续再加字段仍会继续拥挤；需要优先把输入动作测试区改成更稳的布局容器
- 下一步：检查设备页“输入动作测试”这一段的 `egui` 布局，优先把 `滑动起点 / 滑动终点 / 时长` 收敛成不会被截断的行或网格，然后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-16 22:31:15 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：desktop 设备页“输入动作测试”区域里，`滑动起点` 这一行过长，窗口较窄时会显示不全；要求做最小 UI 收口，不改输入动作逻辑
- 新重要记忆：设备页的输入动作测试区不应继续把点按 / 滑动 / 按键三组控件挤在同一个大 `Grid` 里；更稳的做法是按动作分组，每组用自己的小布局容器，避免后续再加字段时整行被压缩截断
- 已完成：已将 `apps/akbox-desktop/src/main.rs` 中的“输入动作测试”从单个 6 列大网格改为三个分组：`点按测试`、`滑动测试`、`按键测试`；其中滑动输入现拆成独立 4 列小网格，`起点 X/Y`、`终点 X/Y`、`时长 ms` 分行展示，并为数值输入框补了固定宽度，避免中文标签和输入框被整行挤爆；输入动作的实际发送逻辑保持不变；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：这次只修了设备页输入测试区的布局，没有继续做 desktop 其他页面的响应式收口；M5 的模板匹配骨架、OCR 封装和视觉调试入口仍按原计划待做
- 风险/阻塞：当前布局已显著缓解“滑动起点一行显示不全”，但如果后续设备页还继续加入更多调试控件，可能仍需要进一步拆成折叠区或更明确的表单段落
- 下一步：回到 M5 主线，继续实现模板匹配骨架和 OCR 封装，并把已有 ROI / artifact 配置接到最小扫描调试入口

### 变更记录

- 日期时间：2026-03-16 22:26:55 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：正式进入 M5，并按最小闭环优先落地“ROI 配置机制 + 页面状态骨架 + 局部截图裁剪 + `scan_artifact` / `recognition_review_queue` 最小写入闭环”；本轮不提前跳到真正 OCR 或模板匹配识别
- 新重要记忆：当前视觉配置的基础语义已固定为 `PageStateCatalog -> PageStateDefinition -> confirmation_markers + rois + supported/recovery actions`；ROI 坐标基于参考分辨率声明，运行时按实际截图尺寸缩放裁剪；低置信度策略当前显式收敛为 `auto_accept / queue_review / reject` 三档，其中 `queue_review` 是默认值
- 已完成：在 `akbox-device` 中新增 `vision.rs`，落地 `PageStateCatalog` / `PageStateDefinition` / `RoiDefinition` / `RoiRect` / `RoiCropResult` / `RoiArtifactPayload`、JSON 配置加载校验、页面动作目标校验，以及 `crop_all_rois_from_png` / `crop_single_roi_from_png` 两个 PNG 局部裁剪入口；在 `akbox-data::repository` 中新增 `ScanArtifactInsert` / `RecognitionReviewQueueInsert`、`insert_scan_artifact` / `enqueue_recognition_review` / `list_*` / `count_*` 接口，并补上相应 record 导出；新增测试覆盖“页面配置指向不存在目标页会报错”“ROI 会按参考分辨率缩放裁剪”“ROI 裁剪产物可写入 `scan_artifact` 且低置信度结果可入 `recognition_review_queue`”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中本轮新增后 `akbox-device` 测试为 `13` 个、`akbox-data` 测试为 `49` 个
- 未完成：模板匹配骨架、OCR 封装、confidence 实际计算还未开始；当前 ROI 配置虽然可从 JSON 加载，但仓库里还没有正式的游戏页面模板文件；这套视觉配置也还没有真正接入 desktop/CLI 的扫描入口
- 风险/阻塞：当前预处理步骤（如灰度、阈值、放大）还只是配置元数据，尚未在裁剪管线中真正执行；因此这一步解决的是“结构和入库”，不是“识别准确率”；另外，若后续页面模板文件命名或目录规范不先定下来，`assets/templates` 很容易再次分散
- 下一步：继续停留在 M5，优先实现模板匹配骨架和 OCR 封装，并把 `vision` 配置接到一个最小扫描调试入口；在真正开始扫描页面前，先约束 `assets/templates` 下的页面模板文件格式和命名规则

### 变更记录

- 日期时间：2026-03-16 22:53:34 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：把当前 CLI 的 `debug vision-inspect` 视觉调试能力正式链接进 desktop GUI，提供独立页面做本地页面配置 / 模板 / PNG 调试，而不是继续混在设备页里
- 新重要记忆：desktop 里的 M5 视觉调试现已固定为一个独立“视觉调试”页，语义与 CLI `vision-inspect` 保持一致：输入页面配置路径、`page_id`、本地 PNG、可选模板根目录和输出目录，后台执行页面确认与 ROI 裁剪，并展示 marker/ROI 摘要和源图预览；当前仍是本地文件调试入口，不直接接 live 设备截图，也不直接写仓库/干员最终态
- 已完成：在 `apps/akbox-desktop` 中新增独立 `Page::Vision` 与 `VisionDebugPageState`，把导航、后台任务轮询、全局 notice 和结果展示接进现有 desktop 主循环；视觉调试页现支持录入页面配置路径、页面 ID、输入 PNG、模板根目录和输出目录，留空时会按 `assets/templates` 和 `debug-artifacts/vision-inspect/<page_id>` 自动推导；后台任务复用 `akbox-device` 的 `load_page_state_catalog_from_path`、`evaluate_page_confirmation_from_png`、`crop_all_rois_from_png` 与 `recognize_text_from_png`，会写出 ROI PNG 和 `manifest.json`，并在 GUI 中展示页面匹配结果、marker 明细、ROI 明细和源图预览；同时为 desktop 补充了模板根目录推导、视觉输出目录推导、请求校验三条纯逻辑测试；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-desktop` 当前测试数为 `13`
- 未完成：GUI 视觉调试目前仍依赖用户手填本地文件路径，没有文件选择器，也没有“一键使用当前设备截图”入口；OCR 仍然只有统一封装和结构化“后端不可用”错误，没有真实后端；视觉调试结果也还没有接入 `scan_artifact` / `recognition_review_queue` 的正式工作流
- 风险/阻塞：虽然 GUI 已能跑通页面确认和 ROI 裁剪，但 OCR 结果在当前阶段会稳定显示为“error / backend unavailable”或“skipped”，不能被误解为识别已经可用；另外，若后续直接在这个页面继续叠加正式扫描流程，容易再次把“调试入口”和“业务扫描入口”混在一起
- 下一步：继续停留在 M5，优先接入真实 OCR 后端或可运行的本地替代识别后端；随后再评估是否把“使用当前设备截图作为视觉调试输入”接进这个页面，最后再把低置信度结果串到 `scan_artifact` / `recognition_review_queue`

### 变更记录

- 日期时间：2026-03-16 22:56:30 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户要求先补全 OCR，让视觉链路先具备完整可运行的识别后端，再继续把 GUI 视觉调试页收口成“非开发者也能用”的入口；这一轮先不要优先做页面配置自动发现或设备截图一键接入
- 新重要记忆：M5 当前顺序已调整为“真实 OCR 后端优先于 GUI 易用性优化”；只有在 `recognize_text_from_png` 不再稳定返回 `backend unavailable` 之后，才继续把视觉调试页从开发入口收口成普通用户入口
- 已完成：已先冻结新的推进顺序，并在开始编码前将该要求写回本文件
- 未完成：尚未开始 OCR 代码实现；当前 `akbox-device::ocr` 仍然只有统一请求/结果类型与结构化占位错误，CLI/desktop 的 OCR 结果仍不可用
- 风险/阻塞：如果这一步直接为 GUI 做大量易用性改造，而 OCR 仍不可用，用户会得到“页面更好点了，但核心识别仍是空壳”的错觉；因此必须先把 Windows 本地 OCR 后端打通，再谈收口
- 下一步：检查 `akbox-device::ocr`、workspace 依赖和当前 Windows 目标环境，优先接入可运行的本地 OCR 后端并补测试；完成后再执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-16 23:35:16 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：按新的 M5 顺序优先补全 OCR，本轮目标是把 `akbox-device::ocr` 从结构化占位错误升级为真实可运行的 Windows 本地 OCR 后端，并确保 CLI / desktop 现有视觉调试链路能直接吃到真实识别结果
- 新重要记忆：当前 OCR 后端已固定为 Windows 自带 `Windows.Media.Ocr`。实现路径为“PNG 解码 -> 必要时缩放到 `OcrEngine::MaxImageDimension` -> 灰度化为 `Gray8` -> `CryptographicBuffer::CreateFromByteArray` -> `SoftwareBitmap::CreateCopyFromBuffer` -> `OcrEngine::RecognizeAsync().get()`”；线程进入 OCR 前会先尝试 `RoInitialize(RO_INIT_MULTITHREADED)`，若线程已在其他 apartment 模式则接受 `RPC_E_CHANGED_MODE` 并继续；`numeric_only` 当前通过本地后处理做数字/全角数字/常见标点归一化
- 已完成：在 workspace 中新增 Windows OCR 所需的 `windows` 依赖与特性，并通过 `target.'cfg(windows)'.dependencies` 接入 `akbox-device`；重写 `crates/akbox-device/src/ocr.rs`，为 Windows 目标接入真实 `Windows.Media.Ocr` 后端，新增语言选择回退（请求语言 -> 用户 Profile 语言 -> 系统首个可用语言）、WinRT apartment 初始化、图像缩放/灰度化、`SoftwareBitmap` 构造和结构化 Windows 错误包装；保留非 Windows 目标的结构化 `Stub` 降级；新增测试覆盖无效 PNG 报错、数字文本归一化、OCR 输入图像缩放，以及“有效 PNG 不再只会报占位错误”；同步放宽了 `apps/akbox-cli` 的 `vision-inspect` 测试断言，使其同时接受真实 OCR `ok` 和无语言环境下的结构化 `error`；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过；另外已做一次命令级 OCR 验收：使用本地生成的数字样例图通过 `cargo run -q -p akbox-cli -- debug vision-inspect ...` 实际跑通，输出 `debug-artifacts/ocr-smoke/out/manifest.json` 中的 OCR 结果为 `backend = windows_native`、`status = ok`、`text = 12345`
- 未完成：GUI 视觉调试页仍然是开发者入口，当前还需要手填路径，没有页面配置自动发现、没有文件选择器、也没有“一键使用当前设备截图”；OCR 虽已接通，但还没有把识别语言、失败原因和用户操作建议收口成普通用户可理解的界面文案
- 风险/阻塞：Windows OCR 的可用语言仍受系统语言包影响；当前实现虽然会自动回退到用户 Profile 或系统首个可用语言，但不同机器上的识别语言可能不完全一致；此外 `hint_text` 仍未真正喂给底层 OCR，因为 `Windows.Media.Ocr` 本身没有 prompt/hint 接口，后续如果要进一步提准，需要结合 ROI 上下文规则而不是指望底层后端
- 下一步：继续停留在 M5，在 OCR 已可运行的前提下把 desktop“视觉调试”页收口成非开发者可用版本，优先做页面配置自动发现/中文下拉选择、隐藏模板根目录等开发字段、接入“使用当前设备截图”，并把结果文案改成玩家可读

### 变更记录

- 日期时间：2026-03-16 23:49:42 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：在 OCR 已补齐后，把 desktop 的“视觉调试”页继续收口成非开发者可用版本；本轮明确要求完成三件事：页面配置自动发现、隐藏模板根目录/输出目录等开发字段、接入“使用当前设备截图”这条运行路径
- 新重要记忆：desktop 的视觉调试页现已改成“双来源 + 模板自动发现”模式：优先从 `assets/templates/pages` 自动发现页面配置并以下拉框展示，默认图像来源改为“使用当前设备截图”，本地 PNG 保留为可选离线调试模式；页面配置路径、`page_id`、模板根目录、输出目录等开发字段已下沉到“高级选项”；当前设备截图模式直接复用 `DevicePageState::build_capture_request` 和 `capture_device_screenshot`，会在输出目录内写入 `device-screenshot.png` 后继续走同一条 `vision-inspect` 识别链
- 已完成：重构 `apps/akbox-desktop/src/main.rs` 中的视觉调试页 UI 和状态机：新增页面模板自动发现 `discover_vision_page_presets`、模板列表刷新、中文页面下拉选择、图像来源模式切换（当前设备截图 / 本地 PNG），以及面向普通用户的运行按钮文案与页面判断提示；高级字段已收敛到折叠区，不再默认暴露；当前设备截图模式已接进后台任务，请求会自动抓取 MuMu / ADB 当前画面并在输出目录落盘后继续执行页面确认 / ROI 裁剪 / OCR；结果区现增加来源说明、人话化页面判断提示和普通用户可读的故障提示；同时新增测试覆盖页面配置自动发现与现有视觉请求校验；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-desktop` 当前测试数为 `14`
- 未完成：当前仓库里仍然没有正式的游戏页面模板文件，`assets/templates/pages` 目前为空，因此普通用户打开视觉调试页时会先看到“未发现页面模板”的提示；这意味着“普通用户流程”已就位，但真正的开箱即用仍依赖后续逐页补齐正式模板；另外当前页仍没有文件选择器，本地 PNG 路径还是文本框
- 风险/阻塞：如果在未随程序提供正式页面模板的情况下直接交付，这一页虽然已经不再要求用户理解模板根目录等开发概念，但仍会因为“没有模板可选”而无法完成真正调试；因此后续要么尽快补一批正式页面模板，要么在产品层面明确把这页标成“高级功能，需先安装模板包”
- 下一步：继续停留在 M5，优先开始补第一批正式页面模板样本并把“使用当前设备截图”进一步收口成真正可点击即用的扫描调试闭环；若模板暂时还不齐，则先补文件选择器/路径记忆，减少本地 PNG 模式下的手工输入成本

### 变更记录

- 日期时间：2026-03-16 23:55:20 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户要求开始补“第一批正式页面模板”，使当前已经可用的 OCR / 视觉调试页不再停留在空模板状态；本轮优先挑选最适合先落地、且能直接通过现有视觉调试页验证的页面模板
- 新重要记忆：补模板这一步不应再只停留在抽象结构，必须真正往 `assets/templates/pages` 和对应 marker 资产目录放入可被 desktop/CLI 自动发现的正式页面模板文件；优先级应偏向“当前能实机抓到、后续扫描流程也会直接用到”的页面
- 已完成：已先冻结新的 M5 子目标，并在开始编码前将该需求写回本文件
- 未完成：模板资产尚未开始补；`assets/templates/pages` 目前仍为空，desktop 视觉调试页依然只能提示“未发现页面模板”
- 风险/阻塞：没有真实截图样本就无法做可信的正式模板；如果硬写坐标和 marker 而不基于真实页面样本，后续模板回收成本会更高
- 下一步：盘点仓库内现有截图 / golden / debug 资产，并结合当前可访问的 MuMu 真机画面，先确定第一批模板页面和样本来源；完成后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-16 23:59:30 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：补第一批正式页面模板，不再让 desktop 视觉调试页停留在“有入口但无模板可选”的状态；本轮先交付能被当前 goldens 和 CLI / GUI 直接消费的正式模板
- 新重要记忆：第一批正式页面模板现已固定落在 `assets/templates/pages`，并必须同时附带对应的 `assets/templates/markers/<page_id>/` marker PNG 与 `assets/golden/vision/` 回归样本；当前首批页面选择为 `inventory_materials_cn`（仓库 / 养成材料子页）和 `operator_detail_status_cn`（干员详情 / 状态页），两者都基于真实设备截图裁切 marker，而不是手写占位图
- 已完成：新增正式页面配置 `assets/templates/pages/inventory_materials_cn.json` 与 `assets/templates/pages/operator_detail_status_cn.json`，并补齐对应 marker 资产 `assets/templates/markers/inventory_materials_cn/{tab_all,tab_materials}.png`、`assets/templates/markers/operator_detail_status_cn/{trust_label,attack_range_label}.png`；新增真实 golden 样本 `assets/golden/vision/inventory_materials_cn.png` 与 `assets/golden/vision/operator_detail_status_cn.png`；更新 `assets/templates/README.md`，把当前内置页面模板说明补出来；在 `crates/akbox-device/src/vision.rs` 中新增两条真实模板回归测试，直接加载正式页面配置和 golden PNG，断言两个页面都能 `matched = true` 且 `matched_markers = 2/2`；另外已做 CLI 级 smoke：`cargo run -q -p akbox-cli -- debug vision-inspect assets/templates/pages/inventory_materials_cn.json inventory_materials_cn assets/golden/vision/inventory_materials_cn.png ...` 与 `cargo run -q -p akbox-cli -- debug vision-inspect assets/templates/pages/operator_detail_status_cn.json operator_detail_status_cn assets/golden/vision/operator_detail_status_cn.png ...` 均返回 `Page matched: yes`、`Marker match count: 2/2`、`ROI output count: 3`；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-device` 当前测试数为 `21`
- 未完成：当前正式模板仍只有两页，还不足以支撑后续仓库扫描 v1 或干员扫描 v1 的完整页面状态机；`inventory_materials_cn` 目前只覆盖仓库里的“养成材料”子页，不等于整个仓库扫描流程；`operator_detail_status_cn` 目前也只覆盖详情页里的状态页，不含技能 / 模组等后续页面
- 风险/阻塞：这批模板目前基于 1920x1080 MuMu 截图样本验证通过，但还没有覆盖更多分辨率、缩放设置或 UI 改版后的回归；另外 `trust_value_numeric` 这类 ROI 目前只是用于调试和 OCR 验证，不应误当成已经足够稳定的最终业务字段读取模板
- 下一步：继续停留在 M5，优先补第二批真正会服务后续扫描主线的正式模板，建议先做“仓库分页 / 结束页判定相关模板”或“干员列表页模板”，让阶段 6 / 阶段 7 的页面状态机有可直接复用的模板基座

### 变更记录

- 日期时间：2026-03-17 00:13:35 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户反馈 desktop 视觉调试页在已补第一批正式模板后，仍然显示“当前还没有可用页面配置，请先刷新页面模板或在高级选项中手工填写”；本轮要求修复模板自动发现，使普通启动方式下也能直接看到内置模板
- 新重要记忆：desktop 当前“页面模板自动发现”存在启动路径耦合风险；如果只按 `current_dir()/assets/templates` 查找模板，那么从 `dist\\方舟看号台.exe`、`target\\debug` 或 IDE 默认工作目录启动时，都可能误判仓库内真实模板不存在。后续模板资产发现必须至少支持“工作目录 / 可执行文件目录 / 仓库根目录”多候选探测，不能继续绑死单一路径
- 已完成：已先确认问题不是模板文件缺失，而是 desktop 侧模板根目录发现策略过窄；并在开始编码前将该修复需求写回本文件
- 未完成：代码尚未开始修改；当前视觉调试页刷新模板时仍只扫描 `working_directory/assets/templates`
- 风险/阻塞：如果仅靠用户手工填高级路径规避，这会直接破坏前一轮“非开发者可用”的目标，而且后续打包路径一变会再次复发
- 下一步：收窄修复范围，优先把 desktop 的模板根目录发现改成多候选自动探测并补测试，覆盖仓库根目录、`dist` 可执行目录和 `target/debug` 可执行目录等常见启动路径；完成后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-17 00:16:11 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：修复 desktop 视觉调试页仍提示“当前还没有可用页面配置”的问题，要求在已有正式模板文件存在时，普通启动方式下也能自动发现它们，而不需要用户手工填写高级路径
- 新重要记忆：desktop 视觉调试页的模板自动发现不能再绑定 `current_dir()`；当前默认策略已固定为“多候选模板根目录探测”：先从工作目录、再从当前可执行文件目录、再从编译期 `apps/akbox-desktop` 所在目录出发，逐级向上寻找存在 `assets/templates/pages` 的目录。这样从仓库根目录、`dist\\方舟看号台.exe`、`target\\debug` 或 IDE 子目录启动时，都能稳定命中同一套正式模板
- 已完成：在 `apps/akbox-desktop/src/main.rs` 中新增 `discover_default_vision_templates_root`、`default_vision_template_search_roots`、`find_vision_templates_root_from_search_roots` 和 `push_unique_path`，并把 `VisionDebugPageState::refresh_page_catalog` 与 `resolved_templates_root` 的默认模板根目录都改为走这条多候选探测逻辑，不再写死 `working_directory/assets/templates`；补充回归测试 `find_vision_templates_root_walks_up_from_nested_start_directory`，覆盖“从嵌套子目录启动，仍能向上命中 `assets/templates`”的场景；同时修正了一个受新自动发现影响的旧测试，使 `vision_state_build_request_rejects_missing_page_id` 在默认已自动填入页面时仍显式验证缺少 `page_id` 的错误分支；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-desktop` 当前测试数为 `15`
- 未完成：这次修的是“找不到模板目录”的根因，不是新增更多页面模板；当前正式模板数量仍只有首批两页。GUI 本轮也没有额外增加文件选择器或模板安装向导
- 风险/阻塞：当前自动发现已覆盖常见开发/本地打包启动路径，但若后续发布版把模板资产放到完全不同的安装目录，仍需要在打包阶段明确模板资产部署位置，或补真正的安装态资源定位策略；否则普通用户环境下仍可能需要更强的“随程序打包模板”约束
- 下一步：继续停留在 M5，回到模板主线补第二批正式页面模板，并在 desktop 里做一次实际手点验收，确认“打开视觉调试页即可看到首批模板”在当前启动方式下已成立；随后再推进仓库分页 / 结束页或干员列表页模板

### 变更记录

- 日期时间：2026-03-17 00:26:08 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：继续补第二批正式页面模板，优先服务阶段 6 的“仓库扫描主线模板”；本轮重点不是再补任意页面，而是优先交付能支撑仓库翻页、重复页判定或结束页判定的模板/ROI
- 新重要记忆：第二批模板要尽量建立在现有真实仓库截图上，避免为了补模板去盲目操控真实游戏界面；在缺少更多实机样本前，应优先落“页面确认 + 可见物品签名 ROI + 翻页前后比较所需样本 ROI”这类低副作用模板，而不是依赖当前界面状态未知的复杂导航页面
- 已完成：已先冻结新的 M5 子目标，并在开始编码前将该需求写回本文件
- 未完成：仓库扫描主线相关模板尚未开始补；当前正式模板仍未覆盖翻页 / 重复页 / 结束页判定所需的页面签名 ROI
- 风险/阻塞：若当前仓库内只有一张仓库页样本，那么本轮更适合先补“可见页签名模板”而不是直接声称完成“结束页模板”；没有连续翻页样本时，不能假装已经验证了真正的末页判断
- 下一步：盘点现有仓库页样本和 probe 产物，优先补一组能直接用于仓库翻页比较的正式 ROI / 模板，并补上 CLI 或单测级验证；完成后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-17 00:28:20 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：继续补仓库扫描主线模板；本轮按“先服务翻页比较、再谈结束页判定”的最小闭环推进，优先交付一份可直接裁出可见页签名 ROI 的正式页面模板
- 新重要记忆：仓库扫描主线在缺少连续翻页样本时，不应伪装成已经完成“末页判定模板”；当前更稳的做法是先把“可见页签名 ROI”正式化，让后续翻页逻辑能对相邻页面做图像签名比较。`inventory_materials_scan_cn` 现已承担这个职责：它复用仓库“养成材料”页已有的顶部 marker，并新增 4 个 `generic` 签名 ROI + 1 个中部数量 OCR 样本 ROI，供后续重复页/结束页比较使用
- 已完成：新增正式页面配置 `assets/templates/pages/inventory_materials_scan_cn.json`，把仓库“养成材料”页的翻页签名 ROI 固定下来；该配置复用 `markers/inventory_materials_cn/{tab_all,tab_materials}.png` 作为页面确认特征，并新增 `signature_count_left`、`signature_count_mid`、`signature_count_right`、`signature_count_far_right` 四个签名 ROI 以及 `signature_count_mid_numeric` 一个辅助数字 OCR ROI；更新 `assets/templates/README.md`，把这份扫描签名模板加入当前内置模板列表；在 `crates/akbox-device/src/vision.rs` 中新增回归测试 `bundled_inventory_scan_template_matches_golden_and_crops_all_signature_rois`，断言该模板能在 `assets/golden/vision/inventory_materials_cn.png` 上 `matched = true` 且成功裁出 5 个 ROI；另外已做 CLI 级 smoke：`cargo run -q -p akbox-cli -- debug vision-inspect assets/templates/pages/inventory_materials_scan_cn.json inventory_materials_scan_cn assets/golden/vision/inventory_materials_cn.png ...` 返回 `Page matched: yes`、`Marker match count: 2/2`、`ROI output count: 5`，且 `signature_count_mid_numeric` 当前实测 OCR 结果为 `6436`；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-device` 当前测试数为 `22`
- 未完成：这一步只补了“仓库翻页比较所需的签名模板”，还没有真正实现翻页后的重复页/结束页判定逻辑；同时也还没有第二张或最后一张仓库页样本，因此还不能声称已经验证了真正的末页行为
- 风险/阻塞：当前签名 ROI 基于单张 1920x1080 仓库样本验证通过，但还没有在连续翻页样本上做“前后页签名确实会变化、末页会稳定重复”的实证；如果后续仓库页布局或卡片间距随分辨率变化明显，还需要补更多样本回归
- 下一步：继续停留在 M5，优先补“仓库翻页前后对比”的第二张样本或模板，然后再把这组签名 ROI 接进阶段 6 的重复页 / 结束页判定逻辑；若当前设备页面可控，再考虑实机抓一组相邻仓库页样本做真正的翻页回归

### 变更记录

- 日期时间：2026-03-17 00:37:39 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：为后续森空岛接口调试在仓库根目录本地新建 `uid.txt`，并确保该文件不会进入版本控制
- 新重要记忆：本地调试用的 UID / 凭据占位文件必须显式加入 `.gitignore`，避免后续为了接口实验把个人标识或敏感调试输入误提交到仓库
- 已完成：已先冻结新的本地文件需求，并在开始编码前将该要求写回本文件
- 未完成：`uid.txt` 尚未创建；`.gitignore` 当前也还没有忽略该文件
- 风险/阻塞：如果只创建文件不加忽略规则，后续很容易在调试森空岛接口时把本地 UID 文件一并提交；反过来如果写进仓库说明文档而不实际创建，也达不到立即可用的目的
- 下一步：在仓库根目录创建 `uid.txt` 并写入本地调试 UID，同时把它加入 `.gitignore`；随后执行 fmt / clippy / test，并回写本文件

### 变更记录

- 日期时间：2026-03-17 00:38:00 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：为森空岛接口后续调试准备本地 UID 文件；要求在仓库根目录创建 `uid.txt`，并确保该文件不会进入版本控制
- 新重要记忆：本地账号 UID 这类调试输入应统一放在仓库根目录显式忽略的本地文件中，避免把个人标识混进配置或源码；当前约定文件名为 `uid.txt`
- 已完成：已在仓库根目录创建 `uid.txt` 并写入本地调试 UID；同时更新 `.gitignore`，新增 `/uid.txt` 忽略规则，确保该文件仅用于本地调试，不会被 Git 跟踪；本轮未改任何 Rust 业务逻辑；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-cli` 25 个测试、`akbox-data` 49 个测试、`akbox-desktop` 15 个测试、`akbox-device` 22 个测试均通过
- 未完成：当前还没有基于该本地 UID 去实际请求森空岛接口；后续若要实测 `player/info`，仍需要你提供本人的有效 `cred` + `token` 或可换取它们的 access token
- 风险/阻塞：`uid.txt` 只解决了本地调试输入存放问题，不解决鉴权问题；仅凭 UID 仍不能直接访问森空岛接口。若后续要接入真实调用，还需要谨慎处理本地凭据存放与签名请求，不应把凭据写入仓库文件
- 下一步：若继续森空岛方向，下一步应准备一个同样本地忽略的凭据输入方案，并在只读模式下验证 `player/info` 对该本地 UID 的实际返回字段；否则就回到 M5/M6 主线，继续补仓库翻页样本与重复页判定逻辑

### 变更记录

- 日期时间：2026-03-17 00:41:10 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户要求 `AGENTS.md` 不得留下隐私信息；当前约束收紧为：不得在 `AGENTS.md` 中写入具体 UID、token、cred、access token 或其他可识别个人账号的信息
- 新重要记忆：`AGENTS.md` 只允许记录“存在本地忽略文件 / 需要本地凭据 / 需要只读验证”这类非敏感流程信息；任何具体个人标识和凭据值都必须留在被 `.gitignore` 忽略的本地文件里，不得进入文档记录
- 已完成：已对最近两条森空岛调试记录做脱敏处理，移除了具体 UID 与带明文 UID 的接口示例，仅保留“本地调试 UID”这类非敏感表述；本轮没有改动业务代码
- 未完成：仓库里其余文档/日志如果后续再引入本地账号调试信息，仍需要继续遵守这条约束；当前只收口了 `AGENTS.md`
- 风险/阻塞：如果后续把森空岛真实凭据调试过程直接写进 issue、设计说明或 commit message，同样会形成隐私泄漏；约束不应只针对 `AGENTS.md`
- 下一步：继续保持 `AGENTS.md` 脱敏；若继续森空岛方向，后续只在本地忽略文件中保存 UID/凭据，并在回复里用泛化描述说明验证步骤

### 变更记录

- 日期时间：2026-03-17 00:40:23 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：继续利用本地忽略的 `uid.txt` 做森空岛接口只读实测，确认在没有本地凭据的情况下，已知 `player/info` / `player/binding` endpoint 对本地 UID 的真实返回
- 新重要记忆：当前已实测确认：仅凭本地 UID 直接请求 `https://zonai.skland.com/api/v1/game/player/info?uid=...` 与 `https://zonai.skland.com/api/v1/game/player/binding`，服务端都会返回 `HTTP 401 Unauthorized`，响应体为 `{\"code\":10002,\"message\":\"用户未登录\"...}`。这说明当前阻塞点是鉴权，不是 endpoint 不存在，也不是 UID 格式错误
- 已完成：已读取本地忽略的 `uid.txt` 并据此对两个已确认存在的森空岛 endpoint 做匿名/缺签名请求；两次请求都稳定返回 `401 + code 10002`；本轮未改任何业务代码，也未把具体 UID 写入本文件
- 未完成：还没有提供带 `cred` / `token` / `sign` 的真实鉴权请求，因此仍无法读取 `player/info` 的实际 JSON 负载
- 风险/阻塞：当前接口鉴权明显收紧，不能再假设“知道 UID 就能查”；如果后续要继续实测，必须在本地忽略文件中准备有效凭据，并谨慎控制请求日志，避免把签名头或账号标识泄漏到仓库
- 下一步：若继续森空岛方向，下一步应准备本地忽略的凭据输入文件，并用只读脚本验证带签名的 `player/info` 是否能返回 `chars / building / status` 等字段；否则就回到 M5/M6 主线继续补仓库翻页样本和重复页判定逻辑

### 变更记录

- 日期时间：2026-03-17 00:41:40 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：尝试构建一组本地鉴权文件约定，用于后续森空岛只读实测；目标是先把文件名、字段和忽略规则固定下来，而不是立刻发起真实鉴权请求
- 新重要记忆：森空岛本地鉴权输入后续应分成“可提交示例模板”和“本地忽略实填文件”两层：前者用于约束字段与顺序，后者用于保存真实凭据；不得把真实 `cred`、`token` 或其他账号标识直接写入仓库跟踪文件
- 已完成：已先冻结新的本地鉴权文件约定需求，并在开始编码前将该要求写回本文件
- 未完成：当前仓库还没有正式的森空岛本地鉴权模板文件，也没有明确的字段说明文档；`.gitignore` 也尚未忽略专用鉴权文件
- 风险/阻塞：如果后续继续沿用临时命令行参数或随手新建文件名，容易导致凭据散落在仓库各处；反过来如果直接把真实凭据写入被跟踪模板文件，又会违反隐私约束
- 下一步：新增一个被 `.gitignore` 忽略的本地鉴权文件名约定、一个可提交的示例模板和一份简短文档，明确字段用途、优先级和隐私边界；随后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-17 00:42:22 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：构建森空岛本地鉴权文件需求组，目标是先固定本地文件结构、字段与忽略规则，为后续只读实测做准备
- 新重要记忆：当前森空岛本地鉴权输入约定已固定为三件套：`uid.txt` 存 UID、`skland-auth.local.toml` 存真实鉴权数据、`skland-auth.local.example.toml` 提供可提交模板；其中只有示例模板允许进版本控制，真实凭据文件必须被 `.gitignore` 忽略
- 已完成：更新 `.gitignore`，新增 `/skland-auth.local.toml` 忽略规则；新增 [skland-auth.local.example.toml](C:/Users/emmmer.SUPERXLB/git/ArkAgent/skland-auth.local.example.toml)，明确 `uid_file`、`cred`、`token`、`user_id`、`access_token` 五个字段的最小模板；新增 [skland-auth-files.md](C:/Users/emmmer.SUPERXLB/git/ArkAgent/docs/skland-auth-files.md)，说明本地文件分层、字段用途和隐私边界；本轮未改任何 Rust 业务逻辑；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-cli` 25 个测试、`akbox-data` 49 个测试、`akbox-desktop` 15 个测试、`akbox-device` 22 个测试均通过
- 未完成：当前还没有实际创建并填写本地忽略的 `skland-auth.local.toml`，因此也还没有带签名去请求真实 `player/info`
- 风险/阻塞：虽然文件约定已经固定，但 `cred` / `token` 的来源和有效期仍是后续最大不确定项；如果本地凭据过期，还需要额外定义刷新或重取流程
- 下一步：若继续森空岛方向，下一步应在本地创建被忽略的 `skland-auth.local.toml` 并填入真实凭据，然后写一个只读验证脚本，优先验证带签名的 `player/info` 是否能稳定返回 `chars / status / building`

### 变更记录

- 日期时间：2026-03-17 00:46:20 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：在 desktop 程序里直接做完整的森空岛扫码登录路径：用户在程序登录界面点击确认登录后，程序生成并展示二维码，用户扫码确认后，程序自动创建或更新本地忽略的 `skland-auth.local.toml`，写入 `access_token / cred / token / user_id`
- 新重要记忆：这条登录闭环当前不需要引入手机端逆向或额外鉴权算法；公开实现已确认扫码链路为 `gen_scan/login -> scan_status -> token_by_scan_code -> oauth2 grant -> generate_cred_by_code`，且 `generate_cred_by_code` 返回的 `CRED` 模型中已包含 `cred`、`token` 与可选 `userId`，因此 desktop 端当前最小闭环可以只依赖这组公开接口完成本地凭据落盘
- 已完成：已先冻结新的 desktop 扫码登录需求，并在开始编码前将该要求写回本文件
- 未完成：当前程序还没有森空岛登录 UI、二维码展示状态机或扫码轮询任务；`skland-auth.local.toml` 也还没有自动写入逻辑
- 风险/阻塞：扫码登录会引入新的后台任务和网络状态机；如果把二维码生成、轮询和文件写入直接塞进现有设置逻辑而不做独立状态管理，GUI 容易再次变得难维护
- 下一步：在 desktop 设置页新增独立“森空岛登录”分组，优先实现“生成二维码 -> 轮询扫码 -> 写入本地鉴权文件”的最小闭环，并补测试覆盖本地鉴权文件读写和默认路径推导；完成后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-17 01:02:22 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：把 desktop 的森空岛扫码登录做成可直接操作的 GUI 闭环：新增独立登录页、点击后弹出二维码、后台轮询扫码状态，并在成功后自动创建或更新本地忽略的 `skland-auth.local.toml`
- 新重要记忆：森空岛扫码登录当前在产品里只定位为“可选的本地只读接口调试辅助”，不能替代 MuMu + ADB + 本地视觉识别主链路；实现上 desktop 已固定走 `gen_scan/login -> scan_status -> token_by_scan_code -> oauth2 grant -> generate_cred_by_code`，并把写盘范围收敛为 `uid_file / access_token / cred / token / user_id` 五个字段，且 UI 与日志都不得回显真实凭据值
- 已完成：已在 `apps/akbox-desktop` 中新增独立 `森空岛登录` 页面和二维码弹窗；登录页支持填写本地鉴权文件路径与 `uid_file` 字段、显示本地鉴权文件状态、启动后台扫码任务、在二维码生成后弹窗展示并持续轮询 `scan_status`；扫码确认后会自动换取 `access_token / cred / token / user_id` 并写入本地忽略的 `skland-auth.local.toml`，不存在时会自动创建；同时新增本地鉴权文件默认路径、读写与缺省值测试。依赖层面为 desktop 补入了 `reqwest / serde / toml / qrcode`
- 未完成：这一步只完成了 GUI 登录与本地凭据落盘，还没有继续接 `player/info`、`binding` 或 box 数据导入；也还没有做真实人工扫码验收，因此当前仍停留在“代码闭环已完成、等待用户实机登录确认”的状态
- 风险/阻塞：森空岛扫码状态码目前只对 `100..=102` 这类“等待扫码/确认中”做了保守轮询兜底；若后续服务端改动状态码或返回结构，可能需要按实际响应补充更精细的错误分支。另外该本地鉴权文件后续若再扩字段，当前重写逻辑不会保留注释
- 下一步：让用户在 desktop 中实际点击 `森空岛登录 -> 确认登录` 完成一次扫码验收；若登录成功，再继续做只读的 `player/info` 调试入口或导入链路，并继续保持“非主方案、不可替代 ADB 主链路”的约束

### 变更记录

- 日期时间：2026-03-17 01:05:31 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户在 desktop 实测森空岛扫码登录时，流程卡在 `generate_cred_by_code`；当前 UI 报错为“解析响应失败：error decoding response body”，要求直接排查并修复响应解码层
- 新重要记忆：实时探针已确认 `https://zonai.skland.com/api/v1/user/auth/generate_cred_by_code` 至少在错误态会返回 `code / message / timestamp` 这类 envelope，而不是先前假设的 `status / msg / data`；因此 desktop 的森空岛请求解码层不能再写死单一 envelope 结构，同时错误提示也不能只暴露模糊的 `error decoding response body`
- 已完成：已先通过真实网络对 `generate_cred_by_code` 做错误态探针，确认当前服务端至少存在 `code + message` 响应形态，并在开始编码前将该修复需求写回本文件
- 未完成：desktop 代码仍然只按 `status / msg / data` 反序列化森空岛响应；若用户再次扫码，到 `generate_cred_by_code` 仍可能继续在解码阶段失败
- 风险/阻塞：如果只修 `generate_cred_by_code` 这一个 endpoint，而不把请求解码收口成兼容 `status/code` 双 envelope 的公共层，后面接 `player/info` 或其他 `zonai` endpoint 时还会再次踩同类问题
- 下一步：把 desktop 森空岛请求解码层改成兼容 `status` 与 `code` 两类 envelope，并在解析失败时回传“HTTP 状态 + 非敏感响应摘要”；补单测后重新执行 fmt / clippy / test，再回写本文件

### 变更记录

- 日期时间：2026-03-17 01:07:16 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：修复 desktop 森空岛扫码登录在 `generate_cred_by_code` 阶段的响应解码失败，并把错误信息改成能反映真实服务端返回而不是泛化的 `error decoding response body`
- 新重要记忆：森空岛相关接口当前不能再假设统一 envelope。`as.hypergryph.com` 这组接口仍然可能返回 `status / msg / data`，而 `zonai.skland.com` 至少已实测存在 `code / message / ...` 形态；因此 desktop 端已把森空岛 HTTP 解码层收口成兼容 `status/code` 双 envelope，并在解析或 HTTP 失败时仅回传“状态码 / message / data keys”这类非敏感摘要，避免把成功态里的 `cred`、`token` 等值暴露到 UI 或日志
- 已完成：已在 `apps/akbox-desktop/src/main.rs` 中把 `SklandApiEnvelope` 改为同时兼容 `status` 与 `code`；`poll_skland_scan_code`、`skland_require_success_data` 与公共 `skland_post_json / skland_get_json` 都已改为走统一 `decode_skland_response`；新增 `summarize_skland_response_body`，在 HTTP 或 JSON 解码失败时输出非敏感响应摘要；补充了三条测试，覆盖 `status` 形态、`code` 形态以及“响应摘要不泄露 token/cred 值”；执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-desktop` 当前测试数为 `21`
- 未完成：这一步修的是响应 envelope 与错误可读性，还没有对真实扫码成功链路做二次人工验收；若服务端成功态 `data` 字段结构还有进一步变化，仍需要按下一次实测结果继续细化
- 风险/阻塞：虽然当前已兼容 `status/code` 双形态，但森空岛网关仍可能偶发返回 `HTTP 5xx + code/message` 或风控页；若后续再出现失败，新 UI 会直接显示更接近真实原因的摘要，便于继续定位
- 下一步：请用户重启 desktop 并再次执行一次森空岛扫码登录；若仍失败，直接根据新的错误摘要继续收窄到具体 endpoint/HTTP 状态。若成功，则继续接 `player/info` 的只读调试入口

### 变更记录

- 日期时间：2026-03-17 01:09:51 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户已在 desktop 中成功完成森空岛扫码登录，并要求立即只读验证“是否可以访问到干员养成情况”
- 新重要记忆：当前本地森空岛鉴权文件虽然落在 `target\\debug` 启动目录下，但配合 `uid.txt` 与公开签名算法，已能稳定访问 `game/player/binding` 与 `game/player/info`；实测 `player/info` 返回的不只是账号概览，还包含 `chars`、`assistChars`、`status`、`building`、`equipmentInfoMap`、`charInfoMap` 等结构，其中 `chars[]` 明确带有 `charId / level / evolvePhase / mainSkillLvl / skills[].specializeLevel / equip[].level / defaultEquipId`，已足够覆盖 v1 干员养成状态的大头
- 已完成：已用本地忽略的森空岛鉴权文件和 `uid.txt` 做只读签名请求验证，成功访问 `https://zonai.skland.com/api/v1/game/player/binding` 与 `https://zonai.skland.com/api/v1/game/player/info?uid=...`；当前绑定角色列表返回成功，`player/info` 也返回成功，并确认存在干员列表、基建信息和模组/专精相关字段。本轮未改 Rust 业务代码，也未把具体 UID、`cred`、`token` 或 `access_token` 写入本文件
- 未完成：当前这一步只是手工只读验证，还没有把森空岛 `player/info` 接进 CLI/desktop 调试入口，也还没有把返回结构映射到本地 `operator_snapshot / operator_state` 模型
- 风险/阻塞：森空岛接口虽然已证明能拿到干员养成主字段，但它仍属于冻结约束里“不能作为主方案的协议逆向私有接口”范畴，因此后续只能作为可选导入/对照源，不能替代 MuMu + ADB + 本地视觉识别主链路；另外当前鉴权文件默认落在启动目录，若继续沿用 `target\\debug` 启动 desktop，需要后续明确更稳定的本地凭据文件定位策略
- 下一步：若继续森空岛方向，优先把这次已验证的 `player/info` 收敛成一个只读 CLI/desktop 调试入口，先输出脱敏字段摘要与 JSON shape，再决定是否做“可选导入到 operator snapshot”的辅助链路；主线阶段上仍继续按 M5/M6 推进视觉扫描

### 变更记录

- 日期时间：2026-03-17 01:11:45 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户正式修改项目约束，明确要求将森空岛接口视为官方高精度数据源，优先级高于 OCR
- 新重要记忆：旧的“协议逆向后的私有接口不能作为主方案”约束对森空岛已不再适用；当前项目口径已调整为：森空岛接口作为《明日方舟》国服的官方主采集源，优先负责干员拥有/养成当前态，MuMu + ADB + 本地视觉识别退为校验、补洞和无接口兜底链路
- 已完成：已在本文件中正式回写这次约束变更：`冻结约束` 里的“协议逆向后的私有接口”已标记为“原约束，森空岛已被替代”；`外部数据源分工` 已新增森空岛并提升为玩家拥有状态第一优先数据源；`数据源优先级` 已改为“用户手工确认 > 森空岛官方接口 > 高置信度本地识别 ...”；`当前重要记忆` 与 `当前待办` 也已同步收口，把森空岛 `player/info` 的只读调试/导入入口提升为高优先级后续工作
- 未完成：这一步只修改了项目约束与路线，没有把代码层的数据导入优先级、扫描流程或 UI 文案全部切到“森空岛优先”
- 风险/阻塞：项目约束虽然已经变更，但代码实现仍然主要围绕 OCR / ADB 扫描构建；如果后续不尽快把森空岛只读调试入口和导入链路落地，文档与实现会继续出现优先级不一致
- 下一步：优先实现森空岛 `player/info` 的只读 CLI / desktop 调试入口，并开始设计“森空岛导入 -> operator snapshot/current state”的主链路；OCR 干员扫描则转为校验与兜底

### 变更记录

- 日期时间：2026-03-17 01:13:45 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：继续落地森空岛主链路，直接实现 `player/info` 的只读 CLI / desktop 调试入口，并开始接 `operator_snapshot / operator_state` 导入链路
- 新重要记忆：这一步的最小闭环不该继续把森空岛逻辑散落在 desktop；既然它已经升为主数据源，`player/info` 请求、签名、脱敏摘要和导入逻辑应尽量下沉到 `akbox-data`，CLI 与 desktop 只做触发和展示，避免后面重复维护两套实现
- 已完成：已先冻结新的实现范围，并在开始编码前将该需求写回本文件
- 未完成：当前森空岛能力仍只存在 desktop 登录页和临时只读脚本里；仓库里还没有正式的 `player/info` 客户端、CLI 调试入口，也没有把返回结构映射到 `operator_snapshot / operator_state`
- 风险/阻塞：如果这一步只在 CLI 或 desktop 临时拼一层展示，而不顺手把导入逻辑接进 repository，下一轮还会再做一遍数据映射；反过来如果一口气做完整 inventory/operator/base 全量导入，又会超出这轮最小闭环
- 下一步：优先在 `akbox-data` 落森空岛 `player/info` 最小客户端与 operator 导入逻辑，再把它接到 CLI `debug` 和 desktop `森空岛登录` 页；完成后执行 fmt / clippy / test 并回写本文件

### 变更记录

- 日期时间：2026-03-17 01:37:00 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：把森空岛 `player/info` 正式收口成可用的只读 CLI / desktop 调试入口，并把干员当前态导入接进 `operator_snapshot / operator_state`
- 新重要记忆：森空岛当前实现已下沉到 `akbox-data`：`player/info` 请求、签名、原始缓存、同步状态、脱敏摘要与 operator 导入都统一经数据层处理；另外本地鉴权文件若落在 `target/debug` 或 `target/release`，`uid_file = "uid.txt"` 现在会按“先同目录，再向上逐级回溯”解析，避免因为启动目录变化导致调试入口误报找不到 UID 文件
- 已完成：新增 `crates/akbox-data/src/skland.rs`，实现森空岛鉴权文件读取、签名 GET、`player/binding` / `player/info` 读取、`raw_source_cache(sync_source_state)` 写入、只读 inspect outcome 和导入 `operator_snapshot / operator_state` 的最小链路；`repository` 已补 `operator_snapshot / operator_state` 的 replace/count/list 接口并加单测；`akbox-cli` 已新增 `debug skland-player-info [--auth-file path] [--database path] [--import]`；desktop 的“森空岛登录”页已新增数据库路径、`检查 player/info`、`导入到干员状态` 两个后台任务按钮与结果摘要展示；新增的 Skland 与 repository 测试均通过。额外 live smoke 结果显示：当前本地真实 `cred/token` 已失效，调用 `game/player/binding` 返回 `401 Unauthorized`，因此若要继续做真实导入验收，需要先重新扫码刷新本地鉴权文件
- 未完成：当前只接了干员当前态导入，还没有继续映射森空岛的库存、基建、理智或提醒相关字段；desktop 也还没有单独的“查看原始 JSON shape / 导出脱敏摘要”入口
- 风险/阻塞：森空岛 `cred/token` 有有效期，live 验收会受到本地凭据是否过期影响；当前代码虽然已经把“找不到 `uid.txt`”这个路径问题修掉，但如果凭据过期，CLI / desktop 仍会直接得到 `401`，需要用户重新扫码
- 下一步：先让用户重新扫码刷新本地鉴权文件，然后用 desktop 的 `检查 player/info` / `导入到干员状态` 做一次真实验收；若通过，再继续把同一套森空岛主链路扩到 inventory / building / alert 所需字段，并重新评估 OCR 在干员侧的收缩范围

### 变更记录

- 日期时间：2026-03-17 01:42:44 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户在 desktop 中点击“检查 player/info”后页面长时间停留在“正在后台执行”，要求直接确认进度并修复卡住问题
- 新重要记忆：desktop 森空岛页当前有两类后台任务：扫码登录任务和 `player/info`/导入任务；轮询逻辑不能把两者串成“先等登录任务完成，否则直接 return”，否则只跑 `player/info` 时 GUI 会一直显示运行中但结果永远不回填
- 已完成：已定位并修复 `apps/akbox-desktop/src/main.rs` 中 `SklandLoginPageState::poll_running_task` 的早退 bug：现在扫码任务与 profile 任务分开轮询，没有登录任务时也会继续处理 `running_profile_task`；并新增回归测试 `skland_profile_task_is_polled_without_login_task`，覆盖“只有 player/info 任务在跑时也能正常收尾”的场景。执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：这一步修的是 GUI 任务收尾，不改变森空岛 live 请求结果；若本地凭据过期，修复后 UI 会结束并显示真实错误，而不是一直卡住
- 风险/阻塞：如果用户当前 desktop 进程仍是修复前启动的旧二进制，界面会继续表现为“卡住”；需要重启 desktop 才能拿到这次修复
- 下一步：让用户重启 desktop 并再次执行 `检查 player/info`；若仍失败，新的界面会直接显示成功摘要或真实错误，再据此继续收窄 live 鉴权/接口问题

### 变更记录

- 日期时间：2026-03-17 02:03:00 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户在重新扫码登录后，desktop 的 `player/info` 检查仍报 `game/player/binding` 401，要求继续收窄 live 鉴权链路直到真实请求可用
- 新重要记忆：当前森空岛 live 鉴权链路还有四个关键约束不能再写错。其一，请求签名串必须使用完整 `request_url` 解析出的 `/api/v1/...` 路径，并把 query 直接拼在路径后面，中间不能额外插入 `?`；其二，`generate_cred_by_code` 返回的初始 token 不能直接长期用于后续签名请求，成功登录后要先调用 `GET /api/v1/auth/refresh` 刷新签名 token；其三，workspace 里的 `reqwest` 必须启用 `gzip`，否则 `player/info` 这类压缩响应会在解码阶段失败；其四，`player/info` 的 `chars[].equip` 实时返回既可能是数组，也可能退化成单个对象，反序列化必须兼容双形态
- 已完成：已在 `apps/akbox-desktop/src/main.rs` 中把扫码登录成功后的 token 链路收口为“`generate_cred_by_code` -> `auth/refresh` -> 写入本地鉴权文件”；在 `crates/akbox-data/src/skland.rs` 中修正了签名串构造、请求前自动 `auth/refresh`、响应文本归一化与非敏感摘要、以及 `equip` 单对象/数组双形态兼容；workspace `reqwest` 已启用 `gzip`；新增/更新了 Skland 相关测试并全部通过。随后执行真实 smoke：`cargo run -q -p akbox-cli -- debug skland-player-info --auth-file target/debug/skland-auth.local.toml --database <temp_db> --import` 已成功返回，确认 `binding` 与 `player/info` 可访问，且 `operator_snapshot / operator_state` 导入链路已打通。最后再次执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过
- 未完成：这一步只收口了森空岛干员当前态的 live 读写链路，还没有继续把 inventory / building / alert 所需字段映射进本地模型，也还没有给 desktop 做更细的导入结果展示
- 风险/阻塞：若用户仍在运行修复前启动的旧 desktop 进程，GUI 仍会继续用旧的签名与旧 token 逻辑，从而继续报 401；需要重启 desktop 才会加载这次修复后的二进制
- 下一步：让用户重启 desktop 后重新执行 `检查 player/info` 或 `导入到干员状态` 做 GUI 验收；若通过，则继续把森空岛主链路向 inventory / building 扩展，并同步收缩干员 OCR 主链路定位

### 变更记录

- 日期时间：2026-03-17 02:18:00 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：用户已确认森空岛干员导入可用，要求把 `player/info` 里的 `building / status` 继续落到本地数据库，并在完成后正式转入 M6 仓库扫描主线
- 新重要记忆：当前最小闭环不应把森空岛基建/账号状态继续只停留在 `raw_source_cache` 或 UI 摘要里；既然森空岛已升为主数据源，这两块也必须采用“当前态表 + 历史快照表”的同一数据库语义，避免后面再把基建当前态硬塞进 `base_layout_config` 这类用户配置表
- 已完成：已在开始编码前确认这轮范围只做“森空岛 `building / status` 最小入库闭环”，不把 M6 仓库扫描一并揉进同一轮大改；同时已完成现有 schema / repository / `player/info` 结构的读取与设计收口
- 未完成：当前仓库里还没有 `player_status_*` 或 `base_building_*` 表，也没有把 `player/info` 导入扩成“干员 + 状态 + 基建”一体更新
- 风险/阻塞：如果这一步直接把森空岛基建状态塞进现有 `base_layout_config` 或只写 `app_meta`，后续做基建提醒和轮班时会把“用户配置”和“实时当前态”混在一起，返工会更大
- 下一步：增加新的 SQLite 表与 repository 接口，把 `player/info` 导入扩成同时更新干员/账号状态/基建当前态，并把 CLI / desktop 摘要与导入结果同步到新的本地状态语义

### 变更记录

- 日期时间：2026-03-17 02:30:00 +08:00
- 阶段：M5 / 阶段 5：视觉基础设施
- 新需求：完成森空岛 `player/info` 中 `building / status` 的最小结构化入库闭环，并让 CLI / desktop 都能触发、查看和验收，然后把下一步切到 M6 仓库扫描
- 新重要记忆：森空岛 `player/info.status` 的实时字段明显比当前最小模型丰富，live 已确认至少存在 `ap / level / mainStageProgress / name / storeTs / uid` 等键；因此这轮先采用“已知摘要字段 + keys_json + raw_json”的保守结构化落库，不假设字段全集固定。基建侧则先收口为 `has_control/meeting/training/hire` 与 `dormitory/manufacture/trading/power/tiredChars` 这些稳定摘要，同时保留 `building_keys_json + raw_json` 供后续扩展
- 已完成：新增 `migrations/0002_skland_status_building.sql`，建立 `player_status_snapshot`、`player_status_state`、`base_building_snapshot`、`base_building_state` 四张表，并在 `AppDatabase` 中接入新 migration；`repository` 新增对应的 replace/count/list 接口和单测；`crates/akbox-data/src/skland.rs` 现已支持提取 `status_keys/storeTs` 与基建摘要，并新增 `import_skland_player_info_into_status_and_building_state`；现有 inspect outcome 也已补齐状态字段和基建摘要。CLI 新增 `akbox-cli debug skland-player-info --import-status-building`；desktop 森空岛页新增“导入账号/基建状态”按钮和结果展示。执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-data` 当前测试数为 `58`。额外 live smoke 也已通过：新的 `--import-status-building` 入口能成功访问真实 `player/info`，并把账号状态/基建当前态写入新表；随后已重新构建 `target\debug\akbox-desktop.exe`
- 未完成：这一步只完成了森空岛账号状态和基建当前态的最小入库，还没有继续把理智提醒、基建疲劳提醒或库存相关字段接到规则层；M6 仓库扫描主线也还没正式开始
- 风险/阻塞：当前 `status` 表仍保留了较多 raw JSON，因为实时字段面还在扩；后续若要把理智、等级、主线进度等字段直接用于提醒或仪表盘，需要再从 `status.raw_json` 中挑出稳定字段做二次结构化。另外 desktop 可执行文件在用户运行时会被 Windows 锁住，更新后仍需要用户重启新二进制才能看到这轮 UI
- 下一步：正式进入 M6 仓库扫描 v1，优先把现有仓库模板和 ROI 接成“翻页 + 重复页判定 + 结束页判定 + `inventory_snapshot / inventory_item_state` 落库”的最小闭环

### 变更记录

- 日期时间：2026-03-17 02:34:00 +08:00
- 阶段：M6 / 阶段 6：仓库扫描 v1
- 新需求：在正式进入 M6 后，先实现“仓库页签名 / 重复页判定基元”这一条最小闭环，为后续翻页与结束页判断提供稳定基础
- 新重要记忆：当前仓库模板里已经有 `inventory_materials_scan_cn`，其中 4 个可见页签名 ROI 和 1 个数字样本 ROI 就是为重复页/结束页判定准备的；因此 M6 的第一步不该重新发明另一套模板体系，而应直接复用这组 ROI 生成页签名，再围绕它做“同页 / 不同页”的稳定比较
- 已完成：已在开始编码前确认这一步只做页签名与重复页判定基元，不提前扩成完整翻页状态机，也不把 `inventory_snapshot` 落库一起塞进同一轮
- 未完成：当前 `akbox-device` 还没有正式的仓库页签名结构、比较函数或基于 ROI PNG 的重复页判定能力
- 风险/阻塞：如果这里直接用整页哈希做重复页判断，后续会因为截图时间、提示气泡或细微像素抖动而误判；必须把判断收口在当前模板定义的稳定 ROI 上
- 下一步：在 `akbox-device` 中新增仓库页签名提取与比较函数，基于 `inventory_materials_scan_cn` 的 ROI 生成稳定签名，并补 golden tests；完成后再继续接翻页动作和 `inventory_snapshot` 落库

### 变更记录

- 日期时间：2026-03-17 02:39:00 +08:00
- 阶段：M6 / 阶段 6：仓库扫描 v1
- 新需求：完成仓库页签名 / 重复页判定的第一个可测试基元，并确保后续翻页逻辑能直接复用，不再依赖整页哈希
- 新重要记忆：M6 当前仓库页签名已固定为“只使用 `inventory_materials_scan_cn` 中 4 个 `Generic` signature ROI”的平均哈希，不把整页像素或重复的 `numeric_ocr` ROI 混进去；比较规则当前按同页模板下逐 ROI 的 Hamming distance 做判定，后续翻页状态机应直接复用这套基元，而不是另起一套截图比较逻辑
- 已完成：已在 `crates/akbox-device/src/vision.rs` 中新增 `InventoryPageSignature`、`InventoryPageSignatureEntry`、`InventoryPageSignatureComparison`、`build_inventory_page_signature` 和 `compare_inventory_page_signatures`，并在 `crates/akbox-device/src/lib.rs` 导出；当前实现会从现有模板裁出 4 个稳定签名 ROI，计算 64-bit 平均哈希，并按逐 ROI Hamming distance 判断是否同页。新增两条 golden tests：同一张正式仓库样例图重复计算时签名一致；对左侧签名 ROI 做明显修改后，比较结果能正确判成“不是同一页”。执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，其中 `akbox-device` 当前测试数为 `24`；另外已重新构建 `target\debug\akbox-desktop.exe`
- 未完成：这一步还只是重复页判定基元，没有把真实翻页动作、结束页检测、扫描循环或 `inventory_snapshot / inventory_item_state` 写库接起来
- 风险/阻塞：当前签名规则只验证了同图一致和明显差异页可区分，还没有实机验证“相邻不同仓库页但局部视觉很像”时的误判率；进入下一步前，最好先用一到两组真实连续翻页截图补充对比样例
- 下一步：继续 M6，基于这套页签名基元实现仓库扫描循环里的“翻页后重复页检测 / 结束页判定”，并开始把单页识别结果汇总成 `inventory_snapshot / inventory_item_state`

### 变更记录

- 日期时间：2026-03-17 02:48:00 +08:00
- 阶段：仓库整理 / 提交前清理
- 新需求：用户要求将本轮所有内容分成几个 commit 推送到远端，并明确要求注意隐私保护
- 新重要记忆：提交前必须再次检查测试、示例文件与 AGENTS 追加记录，确保不会把本地 UID、`cred`、`token`、`access_token` 或本地鉴权文件推到远端；当前已确认 `uid.txt` 与 `target\debug\skland-auth.local.toml` 仍由 `.gitignore` 忽略，另外测试里出现过的真实本地 UID 也必须替换成纯占位值
- 已完成：已把 `crates/akbox-data/src/skland.rs` 测试中的真实本地 UID 全部替换为统一占位 `TEST_UID / SECONDARY_TEST_UID`，避免把用户本地标识推到远端；随后重新执行 `cargo fmt --all`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace` 全部通过，并再次确认 `uid.txt`、`target\debug\skland-auth.local.toml` 与本地 smoke DB 仍被 `.gitignore` 排除
- 未完成：当前还没有真正分组提交和执行 `git push`
- 风险/阻塞：本轮改动跨 M3/M4/M5/M6，`desktop/main.rs`、`cli/main.rs`、`repository.rs` 等文件都叠加了多阶段内容，无法完全按阶段切成绝对纯净的小提交；接下来的 commit 只能按“相对清晰的功能组”整合，而不是逐里程碑无交叉拆分
- 下一步：按功能组整理 commit，提交前再次检查 staged 文件不包含本地敏感文件，然后推送到 `origin/master`

### 变更记录

- 日期时间：2026-03-17 02:58:00 +08:00
- 阶段：仓库整理 / 分组提交
- 新需求：在完成隐私清理后，将本轮改动按相对清晰的功能组提交并推送远端，避免把本地敏感文件或个人标识带入公开历史
- 新重要记忆：当前可稳定落地的提交分组为“数据层与森空岛导入”“设备视觉与模板资产”“CLI/desktop 集成”“AGENTS 文档记录”；其中最后一组只保留里程碑与隐私审计结论，不夹带任何本地鉴权内容
- 已完成：已完成三组功能提交的整理策略确认，并完成其中前两组功能提交与应用层功能提交；当前提交顺序固定为数据层、设备视觉层、应用层，最后再单独提交 `AGENTS.md` 文档收口
- 未完成：`AGENTS.md` 还未单独提交，远端 `origin/master` 也尚未执行 push
- 风险/阻塞：由于本轮跨度大，功能提交之间仍存在少量时间顺序上的交叉背景，但 staged 文件已经按路径与职责隔离；只要继续保持 `AGENTS.md` 单独提交，就不会把文档改动混入功能 commit
- 下一步：单独提交 `AGENTS.md`，复查工作树为空后执行 `git push origin master`
