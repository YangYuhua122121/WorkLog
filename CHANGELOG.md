# Changelog

## v0.2.5 — 2026-10-22

### 修复

- macOS 启动后托盘出现两个图标：托盘只由代码创建一次，移除配置里的 `trayIcon`；
- 托盘对象改为由应用持有，避免被提前回收导致图标消失；
- 单实例判断提前到创建窗口和托盘之前，重复启动不再重复初始化；
- 二次启动时会把已有实例的悬浮球 / 面板唤出。

### Windows

- 安装器不再安装或修复系统 WebView2（`webviewInstallMode: skip`），避免影响系统 EdgeUpdate 注册项；
- 启动日志记录 WebView2 是否已注册，缺失时给出明确提示；
- 如仍提示 `0x8007139F 没有注册类`，请先安装 Microsoft Edge WebView2 Runtime 再启动。

### 打包

- macOS DMG 使用标准大图标布局，带 Applications 快捷方式，App 执行 ad-hoc 签名；
- Release 附带 `SHA256SUMS.txt`。

## v0.2.4 — 2026-10-22

### 打包修复

- macOS DMG 恢复标准大图标布局（应用在左、Applications 在右），仍保留拖拽安装快捷方式；
- macOS 恢复彩色托盘图标，不再显示为白色方块；
- Windows 改用 WebView2 **离线安装器**，修复 `0x8007139F 没有注册类` 导致无法创建 WebView2 的问题；
- 安装包体积会因此增加到约 130 MB，但不再依赖目标机器联网安装 WebView2。

## v0.2.3 — 2026-10-22

### 打包修复

- macOS DMG 内增加「应用程序」快捷方式，可直接拖入 Applications；
- macOS App 在 CI 中执行 ad-hoc 签名，修复部分机器提示应用损坏；
- Windows 安装包内嵌 WebView2 Bootstrapper，缺失运行时时由安装器自动安装；
- 新增启动日志：Windows 为 `%APPDATA%\com.worklog.desktop\startup.log`，macOS 为 `~/Library/Logs/workLog/startup.log`；
- Release 追加 `SHA256SUMS.txt` 便于校验下载完整性。

## v0.2.2 — 2026-10-22

### 新增

- 主面板展开后可从面板本身拖动：顶栏六点把手、顶栏空白处、表格空白处均可拖动整个应用；
- 右键悬浮球弹出原生菜单：展开 / 收起面板、开始 / 停止计时、打开设置、打开数据目录、隐藏到托盘、退出 workLog。

### 优化

- Windows 主面板展开 / 收起改为原生窗口下滑 / 上滑，窗口不透明也能看到完整滑动过程，不再整块突然出现；
- Windows 悬浮球与主面板的层级改为按窗口句柄调整相对顺序，点击主面板使其置前后仍会重新把悬浮球压在上面；
- 悬浮球拖动时不再同步隐藏的主面板；主面板跟随改为单线程按 ~60Hz 取最新位置同步，拖动结束不会丢最后一帧；
- 悬浮球位置保存从「每个 Moved 事件一次 IPC」改为松手 450ms 后只通知一次；
- 移除主面板常驻的 `clip-path + transform + will-change` 合成层，Windows 下交还系统 ClearType，并优先使用 Microsoft YaHei UI；修复文字发虚；
- WebView2 关闭原生遮挡计算（`CalculateNativeWinOcclusion`），避免窗口被判定遮挡后暂停渲染。

## v0.2.1 — 2026-10-21

### 修复

- Windows 主面板改为不透明窗口，修复透明分层导致的明显卡顿与文字发虚；
- Windows 主面板显示 / 重新置顶后，用 `SetWindowPos` 把悬浮球重新提到最前，修复悬浮球被面板遮挡一半；
- Windows 主面板改为直角铺满窗口，避免不透明窗口下圆角露底。

## v0.2.0 — 2026-10-20

### 新增

- 双窗口架构：独立悬浮球窗口 + 固定尺寸主面板窗口；
- macOS 主面板作为悬浮球原生 child window，拖动零延迟跟随；
- 两个窗口开启 `acceptFirstMouse`，非激活状态第一下点击即可操作；
- 主面板从夹子下方滑出的展开 / 收起动画；
- 防遗忘看门狗：正常工作时间之外连续计时提醒与自动停止；
- 统计看板：当日 / 本周 / 本月 / 上周 / 上月，甘特 / 扇形；
- 周 / 月甘特按「项目 × 天」聚合，悬浮查看当天任务清单与时长；
- 报告模板渲染、报告 Skill、OpenAI 兼容接口 AI 改写；
- 深色 / 浅色 / 跟随系统、界面字号；
- 数据表右键导出 CSV / Excel、打开数据库目录、删除任务；
- 计时运行态 / 待保存阶段跨重启恢复（`timer-state.json`）；
- 单实例启动；
- CI（macOS / Windows）与 release workflow。

### 修复

- 桌面端改用 Tauri invoke，不再误用浏览器 mock 数据；
- 修复复合 ID 导致的选中 / 详情 / 计时失效；
- 修复窗口大小、位置、拖动与主面板裁切；
- 修复阶段编辑不落库、自定义字段不持久化；
- 修复计时快停快开时出现双心跳线程。

## v0.1.0 — 2026-10-14

- Tauri 2 工程骨架、SQLite 两张表、项目-任务 CRUD、初版界面移植。
