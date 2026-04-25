#!/usr/bin/env python3
from __future__ import annotations

import argparse
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from cli_common import add_dry_run_arg, add_profile_arg, load_profile_from_args, print_header
from md_board_utils import find_requirement_row, find_section_heading, find_task_row, get_cell
from profile_paths import ProjectPaths

from sync_requirement_pool import sync_requirement_pool_entry
from sync_task_board import sync_task_board_entry


HEADING_RE = re.compile(r"^(#{1,6})\s+(.+?)\s*$")
LIST_ITEM_RE = re.compile(r"^\s*(?:[-*]|\d+\.)\s+(.*)$")
TREE_TOP_RE = re.compile(r"^[├└]──\s*(.+?)\s*$")
TREE_CHILD_RE = re.compile(r"^[│\s]+[├└]──\s*(.+?)\s*$")
BACKTICK_RE = re.compile(r"`([^`]+)`")
BOOK_RE = re.compile(r"《([^》]+)》")
PLACEHOLDER_TEXTS = {"待补充", "待补充。"}
NUMERIC_TITLE_RE = re.compile(r"^\d+(?:\.\d+)*(?:\.)?\s+")
FEATURE_TITLE_RE = re.compile(r"^(F\d{3})\s*(.*)$", re.IGNORECASE)
TABLE_NAME_RE = re.compile(r"`?([a-zA-Z][a-zA-Z0-9_]*)`?")
PLACEHOLDER_KEYWORDS = (
    "待补充",
    "待结合 PRD",
    "待实现",
    "待执行",
    "待排期",
    "TODO",
    "当前为开发前准备稿",
)
REQ_NEW_ID_RE = re.compile(r"^REQ-(\d{8})-(\d+)$")
STRUCTURED_ACCEPTANCE_LABELS = (
    "范围：",
    "优先级：",
    "接口：",
    "代码：",
    "错误码：",
    "一致性：",
    "验收：",
)
OUTDATED_ACCEPTANCE_PHRASES = (
    "评价类型提交后默认状态为已处理且已采纳",
    "依赖 `uk_user_date(user_id, checkin_date)`",
    "uk_user_date(user_id, checkin_date)",
)


@dataclass
class Section:
    level: int
    title: str
    lines: list[str]

    @property
    def text(self) -> str:
        return "\n".join(self.lines).strip()


@dataclass
class GeneratedTask:
    title: str
    acceptance: str
    doc_link: str


@dataclass
class FunctionItem:
    code: str
    module: str
    name: str
    description: str
    priority: str
    detail_points: list[str]
    acceptance_points: list[str]


@dataclass
class FeatureSpec:
    code: str
    title: str
    target: list[str]
    rules: list[str]
    page_requirements: list[str]
    delivery_requirements: list[str]


@dataclass
class TableSpec:
    table_name: str
    purpose: str


@dataclass
class FeatureBlueprint:
    code: str
    module: str
    current_chain: list[str]
    planned_apis: list[str]
    api_contracts: list[str]
    service_methods: list[str]
    error_codes: list[str]
    concurrency_controls: list[str]
    rollback_strategy: list[str]
    table_design: list[str]
    code_touchpoints: list[str]
    task_breakdown: list[str]
    acceptance_steps: list[str]
    sequence_steps: list[str]


DEFAULT_FEATURE_ACCEPTANCE_ALIASES: dict[str, list[str]] = {}

DEFAULT_FEATURE_ACCEPTANCE_ITEMS: dict[str, list[str]] = {}

DEFAULT_FEATURE_TEST_CASE_KEYWORDS: dict[str, list[str]] = {}

DEFAULT_FEATURE_ACCEPTANCE_FALLBACK: dict[str, list[str]] = {}

DEFAULT_SECTION_TITLES: dict[str, tuple[str, ...]] = {
    "background": ("背景与目标", "项目背景", "背景", "背景说明"),
    "goal": ("背景与目标", "产品目标", "目标", "建设目标", "项目范围", "范围定义"),
    "current_state": ("当前代码与数据基线", "当前事实基线", "现状分析", "现状", "当前系统能力"),
    "solution": ("功能概览", "详细需求", "功能详情", "方案设计", "设计方案"),
    "dependencies": ("项目范围", "范围定义", "依赖与前置", "跨需求依赖", "范围边界"),
    "risks": ("风险与待确认", "风险与依赖", "风险", "待确认事项"),
    "scenarios": ("用户与场景", "关键场景", "用户故事", "用户分析", "重点测试场景"),
    "principles": ("核心原则", "设计原则", "原则"),
    "tables": ("数据建议", "数据模型建议", "表结构设计", "当前缺失模型"),
    "function_list": ("功能清单", "功能概览", "功能列表", "功能模块"),
    "acceptance": ("验收标准", "验收口径", "功能验收"),
    "test_cases": ("关键场景", "用户故事", "测试场景", "重点测试场景"),
    "objects": ("数据建议", "数据模型建议", "当前代码与数据基线", "当前缺失模型", "表结构设计"),
}

DEFAULT_TABLE_COLUMN_ALIASES: dict[str, dict[str, tuple[str, ...]]] = {
    "function_list": {
        "code": ("编号", "功能编号", "ID", "Code"),
        "module": ("模块", "功能模块", "Module"),
        "name": ("功能名称", "功能", "Feature", "Name"),
        "description": ("描述", "说明", "Description"),
        "priority": ("优先级", "Priority"),
    },
    "acceptance": {
        "item": ("验收项", "Acceptance Item", "Item"),
        "standard": ("验收标准", "Acceptance Criteria", "Criteria"),
    },
}


def is_tgxsm_project(workspace_root: Path) -> bool:
    # Keep workflow-requirement generic by default.
    # Industry-specific blueprints must not be auto-injected into arbitrary repos.
    return False


def build_feature_blueprints(workspace_root: Path) -> dict[str, FeatureBlueprint]:
    if not is_tgxsm_project(workspace_root):
        return {}

    return {
        "F001": FeatureBlueprint(
            code="F001",
            module="会员权益",
            current_chain=[
                "小程序 `miniprogram/pages/papers/detail/detail.js#downloadPaper` 调用 `api.papers.getPdfAccessToken`。",
                "后端 `PaperController#getPaperById` 已返回 `canDownload`，但仅布尔结果，缺少剩余次数和每日限制。",
                "后端 `FileController#getPdfAccessToken` 与 `accessPdfByToken` 已可发 token/下载，当前仅累计 `papers.download_count`。",
            ],
            planned_apis=[
                "`GET /api/papers/{id}/download-rights`：返回 `canDownload`、`remainingTotal`、`remainingDaily`、`membershipTip`、`limitRule`。",
                "`GET /api/file/token?paperId=`（扩展）: 保留现有签名，响应新增 `remainingTotal`、`remainingDaily`、`downloadRuleVersion`。",
                "`GET /api/file/pdfs/{token}?action=download`（扩展）: 下载成功后除更新 `papers.download_count` 外，同时写 `paper_download_records`。",
            ],
            api_contracts=[
                "`GET /api/papers/{id}/download-rights` 入参：`paperId(path)`；鉴权：`ROLE_USER`；出参：`canDownload:boolean`、`remainingTotal:int`、`remainingDaily:int`、`membershipTip:string`、`limitRule:{dailyLimit,totalLimit,ruleVersion}`。",
                "`GET /api/file/token?paperId=` 入参：`paperId(query)`；出参新增：`remainingTotal`、`remainingDaily`、`downloadRuleVersion`、`tokenExpireAt`。",
                "`GET /api/file/pdfs/{token}?action=download` 成功返回文件流；失败统一返回业务 JSON（含 `code`、`message`、`traceId`）。",
            ],
            service_methods=[
                "`PaperDownloadRightsService#getRights(Integer userId, Integer paperId)`：按手机号聚合会员权益与下载次数。",
                "`PaperDownloadRecordService#createDownloadRecord(Integer userId, Integer paperId, String phone, String token)`：记录下载明细。",
                "`PdfTokenService#generatePdfAccessToken`（扩展）: 注入权益快照，避免前端二次请求。",
                "`FileController#accessPdfByToken` 使用事务边界：下载计数与下载明细同事务提交。",
                "购买会员入口依赖“需求6（购买会员权益详情页调整）”；该依赖已完成并已上线，当前可直接复用。",
            ],
            error_codes=[
                "`DL-001`：无下载权益（会员不存在或已过期）。",
                "`DL-002`：超过当日下载次数限制。",
                "`DL-003`：超过总可下载次数限制。",
                "`DL-004`：下载 token 无效或已过期。",
            ],
            concurrency_controls=[
                "下载校验与扣减在同一事务执行，避免“先校验后更新”产生超发。",
                "依赖 `uk_token_action(token, action)` 保证 token 下载幂等，重复请求仅记一次。",
                "按 `phone + day` 聚合权益使用时，查询口径统一到 DB 时间（避免时区偏差）。",
            ],
            rollback_strategy=[
                "若下载明细写入失败，回滚 `papers.download_count` 更新，整体事务失败。",
                "若文件流发送前发生异常，返回 `DL-004` 并保留 traceId 便于排查。",
                "提供补偿 SQL 模板：按 `token` 对账 `papers.download_count` 与 `paper_download_records`。",
            ],
            table_design=[
                "`paper_download_records`（新增）：`download_id PK`、`user_id`、`phone`、`paper_id`、`token`、`action`、`download_at`、`client_type`、`created_at`。",
                "`paper_download_records` 索引：`idx_pdr_phone_date(phone, download_at)`、`idx_pdr_user_paper(user_id, paper_id)`、`uk_token_action(token, action)`。",
                "`papers`（复用）：保留 `download_count` 作为总量字段；按明细表计算手机号维度权益扣减。",
            ],
            code_touchpoints=[
                "后端：`server/src/main/java/com/juanba/tgxsm/controller/PaperController.java`",
                "后端：`server/src/main/java/com/juanba/tgxsm/controller/FileController.java`",
                "后端：`server/src/main/java/com/juanba/tgxsm/service/PdfTokenService.java`",
                "小程序：`miniprogram/pages/papers/detail/detail.js`",
                "小程序：`miniprogram/utils/api.js`",
            ],
            task_breakdown=[
                "[SQL] 新增 `paper_download_records` DDL 与索引，补回滚脚本。",
                "[后端] 增加下载权益查询服务与 `/api/papers/{id}/download-rights` 接口。",
                "[后端] 扩展 token 与下载接口，落地下载明细与事务一致性。",
                "[小程序] 试卷详情下载区改双态展示（继续下载/购买会员）+ 剩余次数提示。",
                "[小程序] 购买会员按钮直接跳转现网会员详情页，依赖“需求6”已完成并已上线。",
                "[联调] 验证会员关闭、非会员、会员到期、超日限四类场景。",
            ],
            acceptance_steps=[
                "非会员访问详情页：返回 `canDownload=false`，按钮文案为“购买会员”。",
                "会员下载成功后 `paper_download_records` 新增记录且 `papers.download_count` +1。",
                "同手机号跨 openid 下载时，剩余次数口径一致。",
                "点击购买会员可直达现网会员详情页；依赖“需求6”已完成并已上线。",
                "超过每日限制时接口返回明确错误码并给出可读提示。",
            ],
            sequence_steps=[
                "请求详情 -> 查询会员状态/下载权益 -> 返回双态信息。",
                "请求 token -> 校验权益剩余 -> 生成 token + 返回下载 URL。",
                "执行下载 -> 事务内更新 `papers.download_count` + 写下载明细 -> 返回文件。",
            ],
        ),
        "F002": FeatureBlueprint(
            code="F002",
            module="用户运营",
            current_chain=[
                "小程序目前无积分页与积分 API；`miniprogram/utils/api.js` 未定义 points 模块。",
                "后端已有会员与会员卡激活链路：`MembershipService`、`MemberCardActivationService`、`UserActivationController`。",
                "数据库无积分三表，需新增账户/流水/兑换记录。",
            ],
            planned_apis=[
                "`GET /api/points/me`：返回当前总积分、可兑换天数、规则说明。",
                "`GET /api/points/me/details?page=&size=`：积分流水分页。",
                "`GET /api/points/me/redemptions?page=&size=`：兑换记录分页。",
                "`POST /api/points/redeem`：入参 `days`；按 `100积分=1天会员` 扣减并生成待激活权益。",
            ],
            api_contracts=[
                "`GET /api/points/me` 入参：无；出参：`totalPoints`、`maxRedeemDays`、`ruleText`、`pendingActivationDays`。",
                "`GET /api/points/me/details?page=&size=` 入参：分页参数；出参：积分流水列表及分页元数据。",
                "`POST /api/points/redeem` 入参：`days:int`；成功出参：`redeemId`、`costPoints`、`remainingPoints`、`memberCardId`。",
            ],
            service_methods=[
                "`PointsAccountService#getOrCreateAccount(Integer userId)`：账户初始化与余额读取。",
                "`PointsLedgerService#appendDetail(...)`：统一写入积分流水（增加/扣减）。",
                "`PointsRedemptionService#redeem(Integer userId, Integer days)`：事务内扣积分+写兑换记录+生成会员卡。",
                "`PointsRuleService#calcMaxRedeemDays(Integer balance)`：计算默认兑换上限（至少 1 天）。",
            ],
            error_codes=[
                "`PT-001`：积分余额不足。",
                "`PT-002`：兑换天数非法（小于 1 或超过可兑上限）。",
                "`PT-003`：兑换并发冲突，请重试。",
                "`PT-004`：会员卡生成失败，兑换已回滚。",
            ],
            concurrency_controls=[
                "账户扣减使用 `version` 乐观锁（`where user_id=? and version=?`），失败即并发冲突。",
                "兑换事务内顺序：扣积分 -> 写流水 -> 写兑换记录 -> 生成会员卡，任一步失败整体回滚。",
                "`biz_type + biz_id` 组合唯一，防止同一业务重复记账。",
            ],
            rollback_strategy=[
                "会员卡创建失败时回滚积分扣减与兑换记录，返回 `PT-004`。",
                "出现并发冲突时不重试写账，仅返回 `PT-003` 由前端触发重试。",
                "提供按 `redeemId` 对账脚本：账户余额、流水合计、兑换成本三方一致。",
            ],
            table_design=[
                "`user_points_accounts`（新增）：`account_id PK`、`user_id UK`、`phone`、`total_points`、`version`、`created_at`、`updated_at`。",
                "`user_points_details`（新增）：`detail_id PK`、`user_id`、`change_type`、`points_delta`、`biz_type`、`biz_id`、`remark`、`created_at`。",
                "`user_points_redemptions`（新增）：`redemption_id PK`、`user_id`、`days`、`points_cost`、`member_card_id`、`status`、`created_at`。",
                "索引建议：`idx_upd_user_created(user_id, created_at DESC)`、`idx_upr_user_created(user_id, created_at DESC)`。",
            ],
            code_touchpoints=[
                "后端新增：`server/src/main/java/com/juanba/tgxsm/controller/PointsController.java`",
                "后端新增：`server/src/main/java/com/juanba/tgxsm/service/Points*Service.java`",
                "后端复用：`server/src/main/java/com/juanba/tgxsm/service/MemberCardActivationService.java`",
                "小程序新增：`miniprogram/pages/me/points/`（新页面目录）",
                "小程序修改：`miniprogram/pages/me/me/me.js` 与 `miniprogram/utils/api.js`",
            ],
            task_breakdown=[
                "[SQL] 新增积分账户/流水/兑换三表与索引。",
                "[后端] 实现积分账户查询、流水分页、兑换事务接口。",
                "[后端] 兑换后创建待激活会员权益（复用 `member_cards` 与激活链路）。",
                "[小程序] 我的页新增“我的积分”入口与积分详情页。",
                "[联调] 覆盖余额不足、最小兑换 1 天、默认最大可兑天数等场景。",
            ],
            acceptance_steps=[
                "总积分 = 所有流水 `points_delta` 求和，与账户表 `total_points` 对账一致。",
                "兑换 1 天扣减 100 积分，生成待激活权益，不直接写 `memberships`。",
                "兑换接口并发请求仅成功一次（依赖账户版本号或行锁）。",
                "小程序可查看积分明细与兑换记录分页。",
            ],
            sequence_steps=[
                "进入积分页 -> 查询账户/规则/可兑上限 -> 展示默认兑换值。",
                "提交兑换 -> 校验余额与最小天数 -> 事务扣减积分并写兑换记录。",
                "兑换成功 -> 生成待激活权益 -> 返回最新余额与权益信息。",
            ],
        ),
        "F003": FeatureBlueprint(
            code="F003",
            module="用户运营",
            current_chain=[
                "小程序当前无学习打卡页和打卡 API。",
                "后端无 `checkin` 业务控制器和数据模型。",
                "积分能力与打卡奖励存在依赖，需要复用 F002 的积分流水写入能力。",
            ],
            planned_apis=[
                "`GET /api/checkins/calendar?year=YYYY&month=MM`：返回当月打卡日历与统计。",
                "`POST /api/checkins`：今日打卡，成功后返回累计信息和奖励积分。",
                "`GET /api/checkins/summary?year=YYYY&month=MM`：返回本月次数、累计次数、本月完成率。",
            ],
            api_contracts=[
                "`GET /api/checkins/calendar` 入参：`year`、`month`；出参：`days[]`（每日日状态）、`monthCount`、`totalCount`、`completionRate`。",
                "`POST /api/checkins` 入参：无；成功出参：`checkinDate`、`rewardPoints`、`monthCount`、`totalCount`。",
                "`GET /api/checkins/summary` 出参：`monthCount`、`totalCount`、`completionRate`、`todayChecked`。",
            ],
            service_methods=[
                "`StudyCheckinService#checkinToday(Integer userId)`：幂等打卡，限制每天一次。",
                "`StudyCheckinService#getCalendar(Integer userId, int year, int month)`：月历与统计。",
                "`StudyCheckinService#grantCheckinPoints(...)`：调用积分流水服务发奖励。",
                "事务边界：打卡记录写入与积分奖励写入同事务。",
            ],
            error_codes=[
                "`CK-001`：今日已打卡，请勿重复提交。",
                "`CK-002`：月份参数非法。",
                "`CK-003`：积分奖励发放失败，打卡已回滚。",
            ],
            concurrency_controls=[
                "依赖 `uk_study_checkins_user_date(user_id, checkin_date)` 物理唯一约束兜底防重。",
                "先写打卡再写积分流水，二者同事务，避免“打卡成功但未发分”。",
                "打卡接口按 `user_id + checkin_date` 幂等处理，重复请求返回同一业务错误码。",
            ],
            rollback_strategy=[
                "积分流水写入失败时回滚打卡记录并返回 `CK-003`。",
                "若出现唯一键冲突，统一转义为 `CK-001` 而不是数据库异常。",
                "提供按 `checkin_date` 的补偿任务入口，仅针对确认为系统异常的漏发积分场景。",
            ],
            table_design=[
                "`study_checkins`（新增）：`checkin_id PK`、`user_id`、`phone`、`checkin_date`、`points_reward`、`created_at`。",
                "唯一约束：`uk_study_checkins_user_date(user_id, checkin_date)` 防止同日重复打卡。",
                "索引建议：`idx_sc_user_month(user_id, checkin_date)` 支撑月历查询。",
            ],
            code_touchpoints=[
                "后端新增：`server/src/main/java/com/juanba/tgxsm/controller/StudyCheckinController.java`",
                "后端新增：`server/src/main/java/com/juanba/tgxsm/service/StudyCheckinService.java`",
                "小程序新增：`miniprogram/pages/me/checkin/`（新页面目录）",
                "小程序修改：`miniprogram/pages/me/me/me.js` 与 `miniprogram/utils/api.js`",
            ],
            task_breakdown=[
                "[SQL] 新增 `study_checkins` 表与唯一约束。",
                "[后端] 实现打卡接口、月历接口、统计接口。",
                "[后端] 打卡成功后写积分流水，失败时整体回滚。",
                "[小程序] 新增打卡页（统计区 + 月历 + 今日打卡按钮）。",
                "[小程序] 打卡页底部固定提示语“打卡任务可获得积分规则”。",
                "[联调] 覆盖重复打卡、跨月查询、奖励积分到账一致性。",
            ],
            acceptance_steps=[
                "同一用户同一天第二次打卡返回业务错误，不重复发积分。",
                "月历查询可切换年月，统计值与数据库记录一致。",
                "打卡成功后积分流水新增一条 `biz_type=checkin_reward` 记录。",
                "打卡页底部固定提示语与 PRD 一致，不可缺省。",
            ],
            sequence_steps=[
                "进入打卡页 -> 拉取月历与统计 -> 展示本月状态。",
                "点击今日打卡 -> 校验是否已打卡 -> 写入打卡记录并发放积分。",
                "返回最新统计 -> 前端刷新当日状态与累计数据。",
            ],
        ),
        "F004": FeatureBlueprint(
            code="F004",
            module="后台运营",
            current_chain=[
                "后台现有题目反馈页：`admin/src/views/feedback/Feedbacks.vue`。",
                "后端现有题目反馈接口：`QuestionFeedbackController` (`/api/feedbacks`) 与 `question_feedbacks` 表。",
                "小程序现有反馈入口 `pages/feedback/submit` 默认走题目反馈模型。",
            ],
            planned_apis=[
                "`POST /api/user-feedbacks`（ROLE_USER）：提交意见反馈（类型、内容、图片）。",
                "`GET /api/user-feedbacks`（ROLE_ADMIN）：按类型/状态/采纳状态分页查询。",
                "`PUT /api/user-feedbacks/{id}/process`（ROLE_ADMIN）：处理反馈，`adopted` 必填、`reply` 选填。",
                "`PUT /api/user-feedbacks/batch/process`（ROLE_ADMIN）：批量处理。",
            ],
            api_contracts=[
                "`POST /api/user-feedbacks` 入参：`feedbackType`、`content`、`images[]`；出参：`feedbackId`、`status`、`createdAt`。",
                "`GET /api/user-feedbacks` 入参：`feedbackType`、`status`、`adopted`、`page`、`size`；出参：分页列表。",
                "`PUT /api/user-feedbacks/{id}/process` 入参：`adopted:boolean`、`reply?:string(<=100)`；出参：更新后的反馈对象。",
            ],
            service_methods=[
                "`UserFeedbackService#submit(...)`：创建意见反馈，默认状态 `PENDING`，等待后台处理。",
                "`UserFeedbackService#list(...)`：支持筛选与分页。",
                "`UserFeedbackService#process(...)`：单条处理并记录处理人/处理时间。",
                "`UserFeedbackService#batchProcess(...)`：批量更新与失败项返回。",
                "`UserFeedbackSyncService#syncToMiniFeedback(...)`：处理结果反写小程序“我的反馈”列表与详情。",
            ],
            error_codes=[
                "`FB-001`：反馈类型非法或不支持。",
                "`FB-002`：反馈内容为空或超长。",
                "`FB-003`：处理参数非法（`adopted` 缺失）。",
                "`FB-004`：反馈记录不存在或已删除。",
                "`FB-005`：回复内容超过 100 字符。",
            ],
            concurrency_controls=[
                "单条处理使用 `feedback_id` 行级锁或状态版本控制，避免并发覆盖回复内容。",
                "批量处理按单条事务执行并汇总失败项，保证部分成功可追踪。",
                "`question_feedbacks` 与 `user_feedbacks` 分表处理，避免历史题目反馈链路受影响。",
            ],
            rollback_strategy=[
                "批量处理发生单条失败时不中断整体，失败项写入结果明细并回传前端。",
                "处理态更新失败时保留原状态并返回 `FB-004`/`FB-003`，禁止 silent fail。",
                "提供按 `processed_at` 的审计查询，支持回溯管理员处理行为。",
            ],
            table_design=[
                "`user_feedbacks`（新增）：`feedback_id PK`、`user_id`、`feedback_type`、`status`、`is_adopted`、`content`、`images JSON`、`reply`、`processed_by`、`processed_at`、`created_at`、`updated_at`。",
                "索引建议：`idx_uf_status_adopted(status, is_adopted)`、`idx_uf_status_type(status, feedback_type)`、`idx_uf_user_created(user_id, created_at DESC)`。",
                "`question_feedbacks`（保留）：继续承载题目反馈，不做兼容性破坏。",
            ],
            code_touchpoints=[
                "后端保留：`server/src/main/java/com/juanba/tgxsm/controller/QuestionFeedbackController.java`",
                "后端新增：`server/src/main/java/com/juanba/tgxsm/controller/UserFeedbackController.java`",
                "后台改造：`admin/src/views/feedback/Feedbacks.vue`（菜单降级为二级）",
                "后台新增：`admin/src/views/feedback/UserFeedbacks.vue`",
                "小程序改造：`miniprogram/pages/feedback/submit/submit.js`（支持意见反馈类型）",
            ],
            task_breakdown=[
                "[SQL] 新增 `user_feedbacks` 表与状态/采纳索引。",
                "[后端] 新增意见反馈提交、分页、单条处理、批量处理接口。",
                "[后台] 调整菜单层级：新增一级“用户反馈”，下挂“题目反馈/意见反馈”。",
                "[小程序] 反馈提交增加反馈类型与图片提交兼容，处理结果反写“我的反馈”。",
                "[联调] 校验“回复非必填（<=100字）、处理时是否采纳必填、提交后默认 PENDING”。",
            ],
            acceptance_steps=[
                "后台能同时看到题目反馈与意见反馈两个二级菜单。",
                "意见反馈处理时 `is_adopted` 必填，`reply` 可为空且长度不得超过 100 字符。",
                "意见反馈提交后默认状态为 `PENDING`，仅在后台处理后流转到 `PROCESSED` 并回填采纳结果。",
                "后台处理后，小程序“我的反馈”可看到处理状态与回复结果。",
                "批量处理返回成功/失败明细，失败项不影响其他记录。",
            ],
            sequence_steps=[
                "小程序提交意见反馈 -> 后端入库 `user_feedbacks`。",
                "后台列表查询 -> 管理员选择单条或批量处理。",
                "保存处理结果 -> 更新状态/采纳/回复 -> 列表实时反映。",
            ],
        ),
    }


def parse_sections(text: str) -> list[Section]:
    sections: list[Section] = []
    current: Section | None = None
    in_code_block = False

    for raw_line in text.splitlines():
        line = raw_line.rstrip()
        stripped = line.strip()
        if stripped.startswith("```"):
            in_code_block = not in_code_block
        heading = None if in_code_block else HEADING_RE.match(stripped)
        if heading:
            if current is not None:
                sections.append(current)
            current = Section(level=len(heading.group(1)), title=heading.group(2).strip(), lines=[])
            continue
        if current is not None:
            current.lines.append(line)

    if current is not None:
        sections.append(current)
    return sections


def uniq(values: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for value in values:
        item = value.strip()
        if not item or item in seen:
            continue
        seen.add(item)
        result.append(item)
    return result


def normalized_title(title: str) -> str:
    return NUMERIC_TITLE_RE.sub("", title.strip())


def normalize_header(text: str) -> str:
    return text.strip().lower().replace(" ", "")


def tuple_from_config(value: Any, default: tuple[str, ...]) -> tuple[str, ...]:
    if isinstance(value, list):
        items = [str(item).strip() for item in value if str(item).strip()]
        return tuple(items) if items else default
    if isinstance(value, str) and value.strip():
        return (value.strip(),)
    return default


def dict_of_list_from_config(value: Any, default: dict[str, list[str]]) -> dict[str, list[str]]:
    if not isinstance(value, dict):
        return default
    result: dict[str, list[str]] = {key: list(values) for key, values in default.items()}
    for key, raw in value.items():
        if isinstance(raw, list):
            result[str(key)] = [str(item).strip() for item in raw if str(item).strip()]
        elif isinstance(raw, str) and raw.strip():
            result[str(key)] = [raw.strip()]
    return result


def get_section_title_map(profile: dict[str, Any]) -> dict[str, tuple[str, ...]]:
    raw = profile.get("prd_parsing", {}).get("section_titles", {})
    result: dict[str, tuple[str, ...]] = {}
    for key, default in DEFAULT_SECTION_TITLES.items():
        result[key] = tuple_from_config(raw.get(key), default)
    return result


def get_table_column_aliases(profile: dict[str, Any]) -> dict[str, dict[str, tuple[str, ...]]]:
    raw = profile.get("prd_parsing", {}).get("table_columns", {})
    result: dict[str, dict[str, tuple[str, ...]]] = {}
    for table_key, field_defaults in DEFAULT_TABLE_COLUMN_ALIASES.items():
        table_cfg = raw.get(table_key, {}) if isinstance(raw, dict) else {}
        result[table_key] = {}
        for field_key, default in field_defaults.items():
            cfg_value = table_cfg.get(field_key) if isinstance(table_cfg, dict) else None
            result[table_key][field_key] = tuple_from_config(cfg_value, default)
    return result


def get_feature_rule_maps(profile: dict[str, Any]) -> tuple[dict[str, list[str]], dict[str, list[str]], dict[str, list[str]], dict[str, list[str]]]:
    rules = profile.get("prd_parsing", {}).get("feature_rules", {})
    acceptance_aliases = dict_of_list_from_config(rules.get("acceptance_aliases"), DEFAULT_FEATURE_ACCEPTANCE_ALIASES)
    acceptance_items = dict_of_list_from_config(rules.get("acceptance_items"), DEFAULT_FEATURE_ACCEPTANCE_ITEMS)
    test_case_keywords = dict_of_list_from_config(rules.get("test_case_keywords"), DEFAULT_FEATURE_TEST_CASE_KEYWORDS)
    acceptance_fallback = dict_of_list_from_config(rules.get("acceptance_fallback"), DEFAULT_FEATURE_ACCEPTANCE_FALLBACK)
    return acceptance_aliases, acceptance_items, test_case_keywords, acceptance_fallback


def section_matches(section: Section, keywords: tuple[str, ...]) -> bool:
    title = section.title.replace(" ", "")
    return any(keyword.replace(" ", "") in title for keyword in keywords)


def find_sections(sections: list[Section], keywords: tuple[str, ...]) -> list[Section]:
    return [section for section in sections if section_matches(section, keywords)]


def find_section_by_titles(sections: list[Section], titles: tuple[str, ...]) -> list[Section]:
    normalized_targets = {title.strip() for title in titles}
    return [section for section in sections if normalized_title(section.title) in normalized_targets]


def find_feature_sections(sections: list[Section], feature_code: str) -> list[Section]:
    feature_code = feature_code.strip().upper()
    matched: list[Section] = []
    for section in sections:
        title = normalized_title(section.title).upper()
        if title.startswith(feature_code):
            matched.append(section)
    return matched


def list_items_from_sections(sections: list[Section], limit: int = 8) -> list[str]:
    items: list[str] = []
    for section in sections:
        for line in section.lines:
            match = LIST_ITEM_RE.match(line.strip())
            if match:
                items.append(match.group(1).strip())
    return uniq(items)[:limit]


def summary_points(sections: list[Section], limit: int = 6) -> list[str]:
    items = list_items_from_sections(sections, limit=limit)
    if items:
        return items[:limit]

    points: list[str] = []
    for section in sections:
        for line in section.lines:
            stripped = line.strip()
            if not stripped or stripped.startswith("|") or stripped.startswith("```"):
                continue
            points.append(stripped)
            if len(points) >= limit:
                return uniq(points)
    return uniq(points)


def compact_point(text: str, max_len: int = 48) -> str:
    cleaned = re.sub(r"\s+", " ", text.strip().strip("-").replace("`", ""))
    if len(cleaned) <= max_len:
        return cleaned
    for sep in ("。", "；", "，", "：", ":"):
        idx = cleaned.find(sep)
        if 0 < idx <= max_len:
            return cleaned[:idx].strip()
    return cleaned[:max_len].rstrip("，；：: ") + "..."


def extract_feature_detail_points(detail_sections: list[Section], limit: int = 4) -> list[str]:
    points: list[str] = []
    in_code_block = False

    for section in detail_sections:
        for raw_line in section.lines:
            stripped = raw_line.strip()
            if stripped.startswith("```"):
                in_code_block = not in_code_block
                continue
            if in_code_block or not stripped:
                continue
            if stripped.startswith("|") or stripped.startswith("!["):
                continue
            if stripped.startswith("**") and stripped.endswith("**"):
                continue
            if stripped.endswith("：") or stripped.endswith(":"):
                continue
            if stripped.startswith("```"):
                continue

            match = LIST_ITEM_RE.match(stripped)
            candidate = match.group(1).strip() if match else stripped
            if any(keyword in candidate for keyword in ("线框图", "计算公式", "回写规则")):
                continue
            if candidate.startswith(("`", "1.", "2.", "3.", "4.")) and "=" in candidate:
                continue
            compacted = compact_point(candidate)
            if compacted:
                points.append(compacted)

    return uniq(points)[:limit]


def render_bullets(items: list[str], fallback: str) -> str:
    if not items:
        return f"- {fallback}"
    return "\n".join(f"- {item}" for item in items)


def render_sections(sections: list[Section], fallback: str, max_sections: int = 3) -> str:
    meaningful_sections = [section for section in sections if section.text and section.text not in PLACEHOLDER_TEXTS]
    if not meaningful_sections:
        return fallback

    blocks: list[str] = []
    for section in meaningful_sections[:max_sections]:
        body = section.text
        blocks.append(f"### {section.title}\n\n{body}")
    return "\n\n".join(blocks)


def render_fallback_bullets(items: list[str], fallback: str) -> str:
    if items:
        return render_bullets(items, fallback)
    return fallback


def parse_first_table(section: Section | None) -> list[dict[str, str]]:
    if section is None:
        return []

    lines = [line.strip() for line in section.lines]
    for idx in range(len(lines) - 1):
        if not lines[idx].startswith("|") or not lines[idx + 1].startswith("|"):
            continue
        header_cells = [cell.strip() for cell in lines[idx].strip("|").split("|")]
        separator = lines[idx + 1].replace("|", "").replace("-", "").replace(":", "").strip()
        if separator:
            continue
        rows: list[dict[str, str]] = []
        row_idx = idx + 2
        while row_idx < len(lines) and lines[row_idx].startswith("|"):
            row_cells = [cell.strip() for cell in lines[row_idx].strip("|").split("|")]
            if len(row_cells) < len(header_cells):
                row_cells.extend([""] * (len(header_cells) - len(row_cells)))
            rows.append({header_cells[col]: row_cells[col] for col in range(len(header_cells))})
            row_idx += 1
        return rows
    return []


def section_has_table_columns(section: Section | None, aliases: dict[str, tuple[str, ...]]) -> bool:
    rows = parse_first_table(section)
    if not rows:
        return False
    header_keys = {normalize_header(key) for key in rows[0].keys()}
    for field_aliases in aliases.values():
        if not any(normalize_header(alias) in header_keys for alias in field_aliases):
            return False
    return True


def find_section_by_table_columns(sections: list[Section], aliases: dict[str, tuple[str, ...]]) -> Section | None:
    for section in sections:
        if section_has_table_columns(section, aliases):
            return section
    return None


def get_row_value(row: dict[str, str], aliases: tuple[str, ...]) -> str:
    normalized_aliases = {normalize_header(alias) for alias in aliases}
    for key, value in row.items():
        if normalize_header(key) in normalized_aliases:
            return value.strip()
    return ""


def extract_architecture(sections: list[Section]) -> tuple[list[str], dict[str, list[str]]]:
    arch_sections = find_sections(sections, ("功能架构图", "功能架构"))
    top_modules: list[str] = []
    child_map: dict[str, list[str]] = {}

    for section in arch_sections:
        current_top: str | None = None
        for raw_line in section.lines:
            line = raw_line.rstrip()
            top_match = TREE_TOP_RE.match(line)
            child_match = TREE_CHILD_RE.match(line)
            if top_match:
                current_top = top_match.group(1).strip()
                top_modules.append(current_top)
                child_map.setdefault(current_top, [])
                continue
            if child_match and current_top:
                child_map.setdefault(current_top, []).append(child_match.group(1).strip())

    top_modules = uniq(top_modules)
    for key, values in list(child_map.items()):
        child_map[key] = uniq(values)

    if top_modules:
        return top_modules, child_map

    fallback_titles = [
        normalized_title(section.title)
        for section in sections
        if section.level >= 2 and any(keyword in normalized_title(section.title) for keyword in ("功能", "需求", "链路", "场景", "模块"))
    ]
    top_modules = uniq(fallback_titles)[:5]
    return top_modules, {module: [] for module in top_modules}


def extract_object_names(sections: list[Section], limit: int = 8) -> list[str]:
    objects: list[str] = []
    for section in sections:
        objects.extend(BOOK_RE.findall(section.text))
        objects.extend(BACKTICK_RE.findall(section.text))
    filtered = [value for value in uniq(objects) if "md" not in value.lower()]
    return filtered[:limit]


def extract_function_items(
    sections: list[Section],
    function_section: Section | None,
    function_columns: dict[str, tuple[str, ...]],
) -> list[FunctionItem]:
    rows = parse_first_table(function_section)
    items: list[FunctionItem] = []
    for row in rows:
        code = get_row_value(row, function_columns["code"]).strip()
        if not code.startswith("F"):
            continue
        detail_sections = find_feature_sections(sections, code)
        items.append(
            FunctionItem(
                code=code,
                module=get_row_value(row, function_columns["module"]).strip() or "待补充模块",
                name=get_row_value(row, function_columns["name"]).strip() or code,
                description=get_row_value(row, function_columns["description"]).strip() or "待补充功能描述",
                priority=get_row_value(row, function_columns["priority"]).strip() or "待定",
                detail_points=extract_feature_detail_points(detail_sections, limit=4),
                acceptance_points=[],
            )
        )
    return items


def select_feature_subsections(feature_children: list[Section], keyword: str) -> list[Section]:
    return [section for section in feature_children if keyword in normalized_title(section.title)]


def extract_points_from_sections(sections: list[Section], limit: int = 8) -> list[str]:
    if not sections:
        return []
    points = list_items_from_sections(sections, limit=limit)
    if points:
        return points[:limit]
    return summary_points(sections, limit=limit)


def parse_feature_specs(sections: list[Section]) -> list[FeatureSpec]:
    specs: list[FeatureSpec] = []
    for index, section in enumerate(sections):
        matched = FEATURE_TITLE_RE.match(normalized_title(section.title))
        if not matched:
            continue
        code = matched.group(1).upper()
        title = matched.group(2).strip() or code
        end = len(sections)
        for j in range(index + 1, len(sections)):
            if sections[j].level <= section.level:
                end = j
                break
        children = [sections[j] for j in range(index + 1, end) if sections[j].level == section.level + 1]
        target_points = extract_points_from_sections(select_feature_subsections(children, "目标"), limit=4)
        rule_points = extract_points_from_sections(select_feature_subsections(children, "规则"), limit=8)
        page_points = extract_points_from_sections(select_feature_subsections(children, "页面要求"), limit=6)
        delivery_points = extract_points_from_sections(select_feature_subsections(children, "交付要求"), limit=6)
        fallback_points = extract_feature_detail_points([section], limit=8)
        if not target_points and fallback_points:
            target_points = fallback_points[:2]
        if not rule_points and fallback_points:
            rule_points = fallback_points[:4]
        specs.append(
            FeatureSpec(
                code=code,
                title=title,
                target=target_points,
                rules=rule_points,
                page_requirements=page_points,
                delivery_requirements=delivery_points,
            )
        )
    return specs


def table_row_value(row: dict[str, str], aliases: tuple[str, ...]) -> str:
    alias_set = {normalize_header(alias) for alias in aliases}
    for key, value in row.items():
        if normalize_header(key) in alias_set:
            return value.strip()
    return ""


def parse_table_specs(table_sections: list[Section], prd_text: str) -> list[TableSpec]:
    specs: list[TableSpec] = []
    for section in table_sections:
        rows = parse_first_table(section)
        for row in rows:
            table_name = table_row_value(row, ("表名", "目标表", "物理表", "表", "table"))
            purpose = table_row_value(row, ("作用", "说明", "用途", "purpose"))
            if table_name:
                specs.append(TableSpec(table_name=table_name.strip("` "), purpose=purpose or "承接对应业务能力"))

    if not specs:
        for match in TABLE_NAME_RE.finditer(prd_text):
            name = match.group(1)
            if "_" in name:
                specs.append(TableSpec(table_name=name, purpose="PRD提及的目标数据对象"))

    dedup: dict[str, TableSpec] = {}
    for spec in specs:
        key = spec.table_name.lower()
        if key in dedup:
            if not dedup[key].purpose and spec.purpose:
                dedup[key] = spec
            continue
        dedup[key] = spec
    return list(dedup.values())


def merge_function_items(function_items: list[FunctionItem], feature_specs: list[FeatureSpec]) -> list[FunctionItem]:
    merged: dict[str, FunctionItem] = {item.code: item for item in function_items}
    module_hint = function_items[0].module if function_items else "业务功能"
    for spec in feature_specs:
        existing = merged.get(spec.code)
        desc = "；".join((spec.target or spec.rules)[:2]) or f"{spec.code} {spec.title} 需求实现"
        detail_points = uniq((spec.rules + spec.page_requirements + spec.delivery_requirements)[:6])
        if existing is None:
            merged[spec.code] = FunctionItem(
                code=spec.code,
                module=module_hint,
                name=spec.title,
                description=desc,
                priority="P0",
                detail_points=detail_points,
                acceptance_points=[],
            )
            continue
        if existing.name == existing.code and spec.title:
            existing.name = spec.title
        if "待补充" in existing.description and desc:
            existing.description = desc
        if not existing.detail_points and detail_points:
            existing.detail_points = detail_points

    def sort_key(item: FunctionItem) -> tuple[int, str]:
        m = re.search(r"(\d+)", item.code)
        return (int(m.group(1)) if m else 999, item.code)

    return sorted(merged.values(), key=sort_key)


def build_feature_lookup(feature_specs: list[FeatureSpec]) -> dict[str, FeatureSpec]:
    return {spec.code: spec for spec in feature_specs}


def derive_use_cases(use_cases: list[str], feature_specs: list[FeatureSpec]) -> list[str]:
    merged = list(use_cases)
    if merged:
        return uniq(merged)[:10]
    for spec in feature_specs:
        first_rule = (spec.rules or spec.target or [spec.title])[0]
        merged.append(f"{spec.code} {spec.title}：{first_rule}")
    return uniq(merged)[:10]


def derive_principles(principles: list[str], feature_specs: list[FeatureSpec], table_specs: list[TableSpec]) -> list[str]:
    if principles:
        return uniq(principles)[:8]
    generated: list[str] = [
        "业务语义与口径以 PRD 为准，不使用历史实现反推需求。",
        "同一功能点需同时覆盖能力实现、数据落库与追溯证据。",
        "优先复用现有通用能力，新增能力按最小改动落地。",
    ]
    if table_specs:
        generated.append(f"新增/改造数据对象以 `{table_specs[0].table_name}` 等目标表为落地基准，DDL 与索引需同步设计。")
    if feature_specs:
        generated.append(f"任务拆解与验收口径按 {len(feature_specs)} 个 F 编号功能点逐条对齐。")
    return uniq(generated)[:8]


def derive_current_state(current_sections: list[Section], feature_specs: list[FeatureSpec], table_specs: list[TableSpec]) -> list[str]:
    existing = summary_points(current_sections, limit=6)
    if existing:
        return existing
    points: list[str] = [
        "当前仓库已具备部分基础能力，但尚未形成本需求范围的完整闭环。",
        "本次需求需要跨采集、处理、存储、查询与验证链路联动改造。",
    ]
    if table_specs:
        points.append(f"当前数据基线需补齐 {len(table_specs)} 张目标表：{ '、'.join(spec.table_name for spec in table_specs[:6]) }。")
    if feature_specs:
        points.append("功能点需在任务看板和追溯矩阵保持一一对应。")
    return uniq(points)[:8]


def derive_system_change_points(dependency_sections: list[Section], feature_specs: list[FeatureSpec], table_specs: list[TableSpec]) -> list[str]:
    existing = summary_points(dependency_sections, limit=8)
    if existing:
        return existing
    points = [
        "入口层：补齐查询入口与交互展示能力（CLI/Web/API）。",
        "服务层：补齐核心业务处理、校验、异常与回滚策略。",
        "数据层：补齐结构化存储、索引与追溯关系落库能力。",
    ]
    if table_specs:
        points.append(f"数据侧：新增/改造表包括 { '、'.join(spec.table_name for spec in table_specs[:6]) }。")
    if feature_specs:
        points.append("交付侧：每个 F 编号功能至少对应一条任务与一组测试场景。")
    return uniq(points)[:8]


def resolve_acceptance_points(
    item: FunctionItem,
    acceptance_rows: list[dict[str, str]],
    test_cases: list[str],
    acceptance_columns: dict[str, tuple[str, ...]],
    acceptance_items_map: dict[str, list[str]],
    test_case_keyword_map: dict[str, list[str]],
    acceptance_alias_map: dict[str, list[str]],
    acceptance_fallback_map: dict[str, list[str]],
) -> list[str]:
    matched: list[str] = []
    expected_items = acceptance_items_map.get(item.code, [])
    case_keywords = test_case_keyword_map.get(item.code, []) + acceptance_alias_map.get(item.code, [])

    if expected_items:
        for row in acceptance_rows:
            acceptance_item = get_row_value(row, acceptance_columns["item"]).strip()
            acceptance_standard = get_row_value(row, acceptance_columns["standard"]).strip()
            if acceptance_item in expected_items and acceptance_standard:
                matched.append(f"{acceptance_item} {acceptance_standard}")

    for case in test_cases:
        if any(keyword and keyword in case for keyword in case_keywords):
            matched.append(case)

    if not matched:
        matched.extend(acceptance_fallback_map.get(item.code, []))

    return uniq(matched)[:3]


def find_doc(design_dir: Path, keyword: str) -> Path | None:
    matches = sorted(design_dir.glob(f"*{keyword}*.md"))
    return matches[0] if matches else None


def find_testing_doc(testing_dir: Path, keyword: str) -> Path | None:
    matches = sorted(testing_dir.glob(f"*{keyword}*.md"))
    return matches[0] if matches else None


def find_sql_doc(sql_dir: Path, keyword: str) -> Path | None:
    if not sql_dir.exists():
        return None
    matches = sorted(sql_dir.glob(f"*{keyword}*.sql"))
    return matches[0] if matches else None


def is_placeholder_text(content: str) -> bool:
    normalized = re.sub(r"\s+", "", content)
    if len(normalized) < 80:
        return True
    hit_count = sum(1 for keyword in PLACEHOLDER_KEYWORDS if keyword in content)
    return hit_count >= 3


def should_preserve_existing(path: Path, preserve_non_placeholder: bool) -> bool:
    if not preserve_non_placeholder or not path.exists():
        return False
    existing = path.read_text(encoding="utf-8", errors="ignore")
    return not is_placeholder_text(existing)


def content_quality_score(text: str) -> int:
    stripped = text.strip()
    if not stripped:
        return 0
    score = min(len(stripped) // 20, 25)
    score += stripped.count("；")
    score += stripped.count("。")
    score += stripped.count("- ")
    for keyword in PLACEHOLDER_KEYWORDS:
        if keyword in stripped:
            score -= 6
    score += sum(2 for label in STRUCTURED_ACCEPTANCE_LABELS if label in stripped)
    return score


def has_structured_acceptance_markers(text: str) -> bool:
    hit = sum(1 for label in STRUCTURED_ACCEPTANCE_LABELS if label in text)
    return hit >= 4


def has_segmented_acceptance_layout(text: str) -> bool:
    return "范围：" in text and "｜" in text


def choose_richer_text(existing: str | None, generated: str) -> str:
    if not existing:
        return generated
    existing = existing.strip()
    if not existing:
        return generated
    if any(phrase in existing for phrase in OUTDATED_ACCEPTANCE_PHRASES):
        return generated
    if has_segmented_acceptance_layout(generated) and not has_segmented_acceptance_layout(existing):
        return generated
    if has_structured_acceptance_markers(generated) and not has_structured_acceptance_markers(existing):
        return generated
    existing_score = content_quality_score(existing)
    generated_score = content_quality_score(generated)
    if existing_score >= generated_score and not is_placeholder_text(existing):
        return existing
    return generated


def md_link(path: Path, workspace_root: Path) -> str:
    rel = path.relative_to(workspace_root).as_posix()
    return f"[{rel}]({rel})"


def write_text(path: Path, content: str, dry_run: bool, preserve_non_placeholder: bool = False) -> bool:
    if preserve_non_placeholder and path.exists():
        existing = path.read_text(encoding="utf-8", errors="ignore")
        chosen = choose_richer_text(existing, content)
        if chosen == existing:
            print(f"- preserve-existing: {path}")
            return False
        content = chosen
    print(f"- write: {path}")
    if dry_run:
        return False
    path.write_text(content.rstrip() + "\n", encoding="utf-8")
    return True


def update_requirement_section(
    req_file: Path,
    req_id: str,
    background_items: list[str],
    goal_items: list[str],
    doc_links: list[str],
    dry_run: bool,
) -> None:
    if not req_file.exists():
        return

    lines = req_file.read_text(encoding="utf-8").splitlines()
    heading = f"## {req_id}"
    start = find_section_heading(lines, heading)
    if start is None:
        return

    end = len(lines)
    for idx in range(start + 1, len(lines)):
        if lines[idx].startswith("## "):
            end = idx
            break

    block = [
        heading,
        "",
        "### 背景",
        "",
        *(render_bullets(background_items, "待补充。").splitlines()),
        "",
        "### 目标",
        "",
        *(render_bullets(goal_items, "待补充。").splitlines()),
        "",
        "### 关联文档",
        "",
    ]
    block.extend(f"{index}. {link}" for index, link in enumerate(doc_links, start=1))

    if dry_run:
        print("- requirement-section: would update summary block")
        return

    lines[start:end] = block + [""]
    req_file.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")


def build_tech_design(
    doc_date: str,
    theme: str,
    prd_rel: str,
    req_id: str,
    background_sections: list[Section],
    goal_sections: list[Section],
    current_sections: list[Section],
    solution_sections: list[Section],
    dependency_sections: list[Section],
    risk_sections: list[Section],
    principles: list[str],
    current_state_points: list[str],
    system_change_points: list[str],
    use_cases: list[str],
    top_modules: list[str],
    feature_specs: list[FeatureSpec],
) -> str:
    feature_blocks: list[str] = []
    for spec in feature_specs[:8]:
        feature_blocks.extend(
            [
                f"### {spec.code} {spec.title}",
                "",
                "**目标**",
                "",
                *[f"- {item}" for item in (spec.target or ["以 PRD 描述为准，完成对应功能闭环。"])],
                "",
                "**核心规则**",
                "",
                *[f"- {item}" for item in (spec.rules or ["按 PRD 规则实现并可验证。"])],
                "",
                "**页面与交付要求**",
                "",
                *[f"- {item}" for item in (spec.page_requirements + spec.delivery_requirements or ["页面/UI、接口、数据落库与追溯能力需一并交付。"])],
                "",
            ]
        )

    return f"""# {theme} - 技术设计文档

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |
| 需求ID | {req_id} |
| PRD来源 | `{prd_rel}` |

## 2. 背景与问题

{render_sections(background_sections, "待结合 PRD 补充背景与问题说明。")}

## 3. 目标

{render_sections(goal_sections, "待结合 PRD 补充建设目标。")}

### 核心原则

{render_fallback_bullets(principles, "业务语义与口径以 PRD 为准，改造范围覆盖页面、接口、数据与追溯证据。")}

## 4. 现状分析

{render_sections(current_sections, render_bullets(current_state_points, "当前现状待补充。"))}

## 5. 方案设计

{render_sections(solution_sections, "待结合 PRD 补充方案设计。")}

### 模块拆分

{render_bullets(top_modules, "待补充模块拆分。")}

### 功能落地明细

{chr(10).join(feature_blocks) if feature_blocks else "待结合 PRD 继续补充 F 编号功能落地明细。"}

## 6. 系统改造点

{render_sections(dependency_sections, render_bullets(system_change_points, "系统改造点待补充。"))}

## 7. 验收口径

### 核心验收场景

{render_bullets(use_cases, "待补充验收场景。")}

## 8. 风险与依赖

{render_sections(risk_sections, "待结合 PRD 补充风险与依赖。")}
"""


def feature_plan_blocks(
    feature_specs: list[FeatureSpec],
    function_items: list[FunctionItem],
    blueprints: dict[str, FeatureBlueprint],
) -> list[tuple[str, str]]:
    keys: list[str] = []
    keys.extend([spec.code for spec in feature_specs if spec.code])
    keys.extend([item.code for item in function_items if item.code])
    return [(key, key) for key in uniq(keys) if key in blueprints]


def render_feature_blueprint_detail(
    code: str,
    title: str,
    blueprint: FeatureBlueprint,
    fallback_rules: list[str],
    fallback_acceptance: list[str],
) -> str:
    rules = blueprint.task_breakdown or fallback_rules or ["按 PRD 规则实现。"]
    acceptance = blueprint.acceptance_steps or fallback_acceptance or ["按 PRD 场景验收。"]
    api_contracts = blueprint.api_contracts or blueprint.planned_apis or ["接口契约待结合 Controller 入参/出参补齐。"]
    error_codes = blueprint.error_codes or ["错误码待补充（建议按模块编码）。"]
    concurrency_controls = blueprint.concurrency_controls or ["并发控制待补充（乐观锁/唯一约束/幂等）。"]
    rollback_strategy = blueprint.rollback_strategy or ["回滚与补偿策略待补充。"]
    return "\n".join(
        [
            f"### {code} {title}",
            "",
            "**现状调用链（当前项目）**",
            "",
            *[f"- {line}" for line in blueprint.current_chain],
            "",
            "**接口设计（开发实现口径）**",
            "",
            *[f"- {line}" for line in blueprint.planned_apis],
            "",
            "**接口契约（请求/响应）**",
            "",
            *[f"- {line}" for line in api_contracts],
            "",
            "**服务方法与事务边界**",
            "",
            *[f"- {line}" for line in blueprint.service_methods],
            "",
            "**错误码与异常处理**",
            "",
            *[f"- {line}" for line in error_codes],
            "",
            "**并发与一致性控制**",
            "",
            *[f"- {line}" for line in concurrency_controls],
            "",
            "**回滚与补偿策略**",
            "",
            *[f"- {line}" for line in rollback_strategy],
            "",
            "**数据表与索引设计**",
            "",
            *[f"- {line}" for line in blueprint.table_design],
            "",
            "**代码落点（建议文件）**",
            "",
            *[f"- {line}" for line in blueprint.code_touchpoints],
            "",
            "**开发任务拆解（可直接排期）**",
            "",
            *[f"- {line}" for line in rules],
            "",
            "**验收步骤（联调口径）**",
            "",
            *[f"- {line}" for line in acceptance],
            "",
        ]
    )


def build_detailed_design(
    doc_date: str,
    theme: str,
    prd_rel: str,
    top_modules: list[str],
    child_map: dict[str, list[str]],
    function_items: list[FunctionItem],
    objects: list[str],
    dependency_sections: list[Section],
    risk_sections: list[Section],
    use_cases: list[str],
    feature_specs: list[FeatureSpec],
    blueprints: dict[str, FeatureBlueprint],
) -> str:
    module_rows = ["| 模块 | 职责 | 关键子项 |", "| --- | --- | --- |"]
    if function_items:
        module_map: dict[str, list[str]] = {}
        for item in function_items:
            module_map.setdefault(item.module, []).append(f"{item.code} {item.name}")
        for module, features in list(module_map.items())[:8]:
            children = "；".join(features[:4])
            module_rows.append(f"| {module} | 承接 `{prd_rel}` 中对应模块的实现与联调 | {children} |")
    else:
        for module in top_modules[:6]:
            children = "；".join(child_map.get(module, [])[:4]) or "按 PRD 细化"
            module_rows.append(f"| {module} | 承接 `{prd_rel}` 中对应模块的实现与联调 | {children} |")

    object_bullets = render_bullets(objects, "待补充关键业务对象。")
    feature_bullets = [f"{item.code} {item.name}：{item.description}" for item in function_items[:8]]
    feature_details: list[str] = []
    item_map = {item.code: item for item in function_items}
    spec_map = {spec.code: spec for spec in feature_specs}
    blueprint_blocks = feature_plan_blocks(feature_specs, function_items, blueprints)

    if blueprint_blocks:
        for code, _ in blueprint_blocks[:8]:
            item = item_map.get(code)
            spec = spec_map.get(code)
            title = item.name if item and item.name else (spec.title if spec else code)
            fallback_rules = item.detail_points if item else (spec.rules if spec else [])
            fallback_acceptance = item.acceptance_points if item else []
            feature_details.append(
                render_feature_blueprint_detail(
                    code,
                    title,
                    blueprints[code],
                    fallback_rules,
                    fallback_acceptance,
                )
            )
    else:
        for item in function_items[:8]:
            feature_details.extend(
                [
                    f"### {item.code} {item.name}",
                    "",
                    f"- 业务描述：{item.description}",
                    f"- 模块归属：{item.module}",
                    f"- 优先级：{item.priority}",
                    f"- 规则拆解：{'；'.join(item.detail_points[:5]) or '按 PRD 子章节实现'}",
                    f"- 验收要点：{'；'.join(item.acceptance_points[:3]) or '按 PRD 场景与规则验收'}",
                    "",
                ]
            )

        if not feature_details and feature_specs:
            for spec in feature_specs[:8]:
                feature_details.extend(
                    [
                        f"### {spec.code} {spec.title}",
                        "",
                        f"- 核心规则：{'；'.join(spec.rules[:5]) or '按 PRD 规则执行'}",
                        f"- 页面要求：{'；'.join(spec.page_requirements[:4]) or '页面与交互需符合 PRD 描述'}",
                        f"- 交付要求：{'；'.join(spec.delivery_requirements[:4]) or '接口、数据与追溯需同步交付'}",
                        "",
                    ]
                )

    return f"""# {doc_date} 详细开发设计 - {theme}

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |
| PRD来源 | `{prd_rel}` |

## 2. 模块拆分

{chr(10).join(module_rows)}

## 3. 数据流与时序

1. 请求进入 Controller，先完成身份与参数校验（含会员开关、手机号与状态校验）。
2. Service 层执行业务规则判断，命中异常立即短路返回业务错误码。
3. 在事务中执行“状态更新 + 明细落库 + 追溯记录”，确保一致性。
4. 返回调用方所需的聚合字段，便于入口层直接渲染或展示。

## 4. 核心对象设计

{object_bullets}

## 5. 接口与方法设计

### 预期入口

{render_bullets(feature_bullets or top_modules, "待补充接口与方法。")}

## 6. SQL 与数据落库设计

{render_sections(dependency_sections, "待结合 PRD 补充数据落库设计。", max_sections=2)}

## 7. 异常处理策略

{render_sections(risk_sections, "待结合 PRD 补充异常处理策略。", max_sections=2)}

## 8. 测试与验证设计

{render_bullets(use_cases, "待补充测试与验证设计。")}

## 9. 功能点级实现说明

{chr(10).join(feature_details) if feature_details else "待结合 PRD 功能点继续补充实现说明。"}
"""


def build_breakdown(
    doc_date: str,
    theme: str,
    design_rel: str,
    detailed_design_rel: str,
    prd_rel: str,
    function_items: list[FunctionItem],
    top_modules: list[str],
    child_map: dict[str, list[str]],
    use_cases: list[str],
    feature_lookup: dict[str, FeatureSpec],
    blueprints: dict[str, FeatureBlueprint],
) -> str:
    tasks: list[str] = []
    source_items = function_items or [
        FunctionItem(code=f"T{index:03d}", module=module, name=module, description=module, priority="P0", detail_points=child_map.get(module, []), acceptance_points=[])
        for index, module in enumerate((top_modules[:4] or ["数据模型与配置", "后端核心能力", "前后端页面与交互", "测试与验收"]), start=1)
    ]
    for index, item in enumerate(source_items, start=1):
        child_text = "；".join(item.detail_points[:4]) or item.description
        spec = feature_lookup.get(item.code)
        extra_scope = "；".join((spec.delivery_requirements if spec else [])[:4])
        acceptance_points = item.acceptance_points or ([use_cases[index - 1]] if index - 1 < len(use_cases) else [f"完成 {item.name} 对应能力并可验证"])
        blueprint = blueprints.get(item.code)
        touchpoints = blueprint.code_touchpoints if blueprint else []
        task_points = blueprint.task_breakdown if blueprint else []
        api_points = blueprint.planned_apis[:3] if blueprint else []
        service_points = blueprint.service_methods[:3] if blueprint else []
        tasks.extend(
            [
                f"### 3.{index} {item.code} {item.name}",
                "",
                "目标：",
                f"- 依据 `{prd_rel}` 落实 `{item.code} {item.name}` 对应能力。",
                "",
                "改动范围：",
                f"- 设计依据：`{design_rel}`",
                f"- 详细设计：`{detailed_design_rel}`",
                f"- 模块：{item.module}",
                f"- 关键子项：{child_text}",
                f"- 交付要求：{extra_scope or '接口、页面、数据模型与追溯文档同步交付'}",
                *([f"- 接口落地：{line}" for line in api_points] if api_points else []),
                *([f"- 服务落地：{line}" for line in service_points] if service_points else []),
                *([f"- 代码文件：{line}" for line in touchpoints[:5]] if touchpoints else []),
                *([f"- 子任务：{line}" for line in task_points[:5]] if task_points else []),
                "",
                "验收重点：",
                *[f"- {point}" for point in (blueprint.acceptance_steps[:4] if blueprint and blueprint.acceptance_steps else acceptance_points)],
                f"- 对齐 PRD 描述：{item.description}",
                "",
            ]
        )

    tasks.extend(
        [
            f"### 3.{len(source_items) + 1} 联调与测试验证",
            "",
            "目标：",
            "- 按 UAT/联调计划完成关键场景验证，补齐自动化和手工证据。",
            "",
            "改动范围：",
            "- 联调验收记录",
            "- 测试结果",
            "- UAT 测试用例",
            "",
            "验收重点：",
            f"- {use_cases[-1] if use_cases else '关键业务场景已覆盖'}",
            "",
        ]
    )

    return f"""# {doc_date} 开发任务拆解 - {theme}

## 1. 对应需求

- PRD：`{prd_rel}`
- 技术设计：`{design_rel}`
- 详细开发设计：`{detailed_design_rel}`

## 2. 总体原则

1. 业务语义以 PRD 为准，不用历史实现反推需求。
2. 每条任务只对应一个明确交付物或 PRD 缺口。
3. 任务完成后必须同步更新追溯、测试或联调文档。

## 3. 任务拆解

{chr(10).join(tasks)}

## 4. 推荐执行顺序

1. 先完成数据模型、配置和依赖校验。
2. 再实现核心后端能力与接口。
3. 再实现入口层交互与查询能力，最后执行联调和测试验证。
"""


def build_physical_design(doc_date: str, theme: str, objects: list[str], table_names: list[str], table_specs: list[TableSpec]) -> str:
    table_hint_map: dict[str, tuple[str, str, str, str]] = {}

    purpose_map = {spec.table_name.lower(): (spec.purpose or "承接对应业务数据") for spec in table_specs}
    ordered_tables = uniq([spec.table_name for spec in table_specs] + table_names)
    if not ordered_tables:
        ordered_tables = [
            "projects",
            "items",
            "tasks",
            "decisions",
            "evidence",
            "links",
        ]

    rows: list[str] = []
    for table in ordered_tables[:12]:
        key = table.strip("` ").lower()
        field_hint, index_hint, default_hint, module_hint = table_hint_map.get(
            key,
            (
                "按业务主键、状态字段、审计字段补齐（id/user_id/status/created_at/updated_at）",
                "按查询场景补充唯一约束与组合索引",
                "状态与枚举建议配置化，避免硬编码",
                "通用业务承接",
            ),
        )
        purpose = purpose_map.get(key, module_hint)
        rows.append(f"| {table} | {field_hint} | {index_hint} | {default_hint} | {purpose} |")

    existing_rows = "\n".join(rows)
    object_notes = render_bullets(objects, "对象名以 PRD 术语为准，落库前需对齐表名。")
    execute_order = "\n".join(f"{idx}. `{table}`" for idx, table in enumerate(ordered_tables[:12], start=1))
    return f"""# {doc_date} 物理表设计 - {theme}

## 1. 文档信息

| 项目 | 内容 |
| --- | --- |
| 文档日期 | {doc_date} |
| 需求主题 | {theme} |

## 2. 现有表扩展

| 表名 | 关键字段建议 | 约束/索引建议 | 默认值/枚举建议 | 说明 |
| --- | --- | --- | --- | --- |
{existing_rows}

## 3. 新增表设计

{object_notes}

## 4. DDL 执行顺序（建议）

{execute_order}

## 5. 唯一键与索引核对清单

- 覆盖“唯一性、防重、分页、按时间倒序查询”四类索引场景。
- 关键业务链路均需具备“按实体 ID + 时间”查询索引。
- 涉及事务幂等的链路必须有唯一约束（例如 token/action、user/date、biz_type/biz_id）。

## 6. 约束与备注

- 本设计用于开发实现阶段，字段长度和索引前缀需结合现网数据量最终落库。
- 涉及历史表变更时必须做向前兼容评估。
"""


def build_table_mapping(doc_date: str, theme: str, objects: list[str], table_names: list[str], table_specs: list[TableSpec]) -> str:
    module_map: dict[str, str] = {}
    rows = [
        "| PRD对象/术语 | 对应功能 | 目标表 | 映射策略 | 备注 |",
        "| --- | --- | --- | --- | --- |",
    ]
    if table_specs:
        for spec in table_specs[:12]:
            key = spec.table_name.lower()
            feature = module_map.get(key, "待归属功能")
            strategy = "新增表" if key in {"paper_download_records", "user_points_accounts", "user_points_details", "user_points_redemptions", "study_checkins", "user_feedbacks"} else "复用现网表"
            rows.append(f"| {spec.purpose or spec.table_name} | {feature} | {spec.table_name} | {strategy} | 需与接口返回字段保持一一映射 |")
    else:
        candidates = objects[:8] or ["项目", "知识条目", "任务", "决策", "证据", "实体关联"]
        for index, name in enumerate(candidates):
            table = table_names[index] if index < len(table_names) else "待确认表"
            feature = module_map.get(table.lower(), "待归属功能")
            rows.append(f"| {name} | {feature} | {table} | 待确认（新增或复用） | 需在 DDL 评审前冻结 |")
    return f"""# {doc_date} 表名对照表 - {theme}

## 1. 设计原则

- 优先遵循“一个 PRD 对象对应一张主承接表”的映射原则。
- 新增表用于承接新业务语义，复用表只做兼容字段或关联补充。
- 接口字段、DTO 字段、数据库字段命名保持同一语义。

## 2. 对照关系

{chr(10).join(rows)}

## 3. 映射检查清单

- 接口返回字段是否可直接追溯到目标表字段。
- 是否存在“一条业务写多表但未定义主表”的情况。
- 复用历史表是否明确了兼容策略与回滚方案。
"""


def build_prd_trace(
    doc_date: str,
    theme: str,
    prd_rel: str,
    design_rel: str,
    detailed_design_rel: str,
    breakdown_rel: str,
    uat_rel: str,
    top_modules: list[str],
    child_map: dict[str, list[str]],
    feature_specs: list[FeatureSpec],
    blueprints: dict[str, FeatureBlueprint],
) -> str:
    def trace_desc(points: list[str], fallback: str) -> str:
        cleaned: list[str] = []
        for point in points:
            text = point.strip()
            if not text:
                continue
            if text.endswith(("：", ":")):
                continue
            if text in {"列表字段包含", "列表字段包含："}:
                continue
            if len(text) <= 6:
                continue
            cleaned.append(text)
        if cleaned:
            return "；".join(cleaned[:4])
        return fallback

    rows = ["| PRD功能点 | 规则摘要 | 设计文档 | 任务拆解 | 测试文档 | 当前状态 | 下一动作 |", "|----------|----------|----------|----------|----------|----------|----------|"]
    if feature_specs:
        for spec in feature_specs[:10]:
            blueprint = blueprints.get(spec.code)
            source_points = (
                (blueprint.acceptance_steps[:4] if blueprint and blueprint.acceptance_steps else [])
                or spec.rules
                or spec.target
            )
            raw_desc = "；".join((source_points or [])[:6]) or spec.title
            desc = trace_desc(source_points or [], raw_desc)
            rows.append(
                f"| {spec.code} {spec.title} | {desc} | `{design_rel}`<br>`{detailed_design_rel}` | `{breakdown_rel}` | `{uat_rel}` | 设计已完成，待编码 | 开发完成后回填代码路径与测试结果 |"
            )
    else:
        modules = top_modules[:6] or ["核心需求"]
        for module in modules:
            desc = "；".join(child_map.get(module, [])[:4]) or module
            rows.append(
                f"| {module} | {desc} | `{design_rel}`<br>`{detailed_design_rel}` | `{breakdown_rel}` | `{uat_rel}` | 需求已入池，待模块开发 | 按任务看板推进并回填证据 |"
            )
    return f"""# {doc_date} PRD追溯 - {theme}

## 文档信息

| 项目 | 内容 |
|------|------|
| 需求主题 | {theme} |
| 对应PRD | `{prd_rel}` |
| 创建日期 | {doc_date} |

---

## 需求追溯矩阵

{chr(10).join(rows)}

---

## 上线前必回填项

- [ ] 每个 F 编号功能至少回填 1 个后端代码入口和 1 个前端/后台入口。
- [ ] 每个 F 编号功能至少回填 1 条联调证据（接口响应/SQL核对/页面截图之一）。
- [ ] 测试结果文档需补充“实际结果/证据/是否通过”。

---

## 确认结论

- [x] 已建立 PRD -> 设计 -> 任务 -> 测试 的追溯链路。
- [ ] 当前阶段为“设计完成，待开发执行”。
- [ ] 进入提测前需完成“代码证据 + 测试证据”双回填。
"""


def build_product_confirmation(
    doc_date: str,
    theme: str,
    prd_rel: str,
    confirmation_items: list[str],
    feature_specs: list[FeatureSpec],
    blueprints: dict[str, FeatureBlueprint],
) -> str:
    feature_map = {spec.code: spec for spec in feature_specs}
    blocker_items: list[dict[str, Any]] = []
    non_blocker_items: list[dict[str, Any]] = []

    def add_item(
        bucket: list[dict[str, Any]],
        feature: str,
        title: str,
        question: str,
        impact: str,
        default_decision: str,
        decisions: list[str],
        fallback_action: str,
        owner: str = "产品经理",
        due: str = "提测前冻结",
        options: list[str] | None = None,
    ) -> None:
        bucket.append(
            {
                "feature": feature,
                "title": title,
                "question": question,
                "impact": impact,
                "default_decision": default_decision,
                "decisions": decisions,
                "fallback_action": fallback_action,
                "owner": owner,
                "due": due,
                "options": options or [],
            }
        )

    if "F001" in feature_map:
        add_item(
            blocker_items,
            "F001 下载限制",
            "下载阈值口径冻结",
            "会员总额度、每日限制、是否分会员档位是否固定并配置化？",
            "影响下载权益计算、前端提示文案、联调验收标准与线上配置策略。",
            "阈值走配置：`memberTotalLimit=9999`、`dailyLimit=3`；购买会员入口依赖“需求6”已完成并上线，可直接复用。",
            [
                "会员总额度默认值（整数）",
                "每日限制默认值（整数）",
                "是否按会员档位差异化（是/否）",
            ],
            "先按默认配置口径开发并联调，提测前冻结最终配置值。",
            options=[
                "方案A（推荐）：统一阈值 + 配置化（memberTotalLimit/dailyLimit）",
                "方案B：按会员档位差异化阈值（需补完整映射表）",
            ],
        )

    if "F002" in feature_map:
        add_item(
            blocker_items,
            "F002 用户积分链路",
            "积分来源映射与上限口径",
            "除兑换规则 `100积分=1天会员` 外，打卡/反馈等积分来源映射是否冻结？是否有每日积分上限？",
            "影响积分流水 `change_reason`、积分对账口径、前端规则展示与测试用例设计。",
            "先完成配置化：`points_rule.exchangeRate=100`、`checkinReward=2`，其余映射走配置表。",
            [
                "打卡奖励积分（整数）",
                "反馈类型 -> 积分映射（枚举表）",
                "是否存在每日积分上限（是/否）",
            ],
            "先按默认配置口径落地，产品确认后仅调配置不改代码。",
            options=[
                "方案A（推荐）：配置化积分映射 + 每日上限关闭",
                "方案B：配置化积分映射 + 每日上限开启（需补上限值）",
            ],
        )

    if "F003" in feature_map:
        add_item(
            blocker_items,
            "F003 学习打卡",
            "打卡刷新时区与奖励发放口径",
            "“每日凌晨刷新”以服务端时区还是用户时区为准？奖励积分是否允许按活动期调整？",
            "影响重复打卡判定、跨时区用户体验、月历统计准确性和积分一致性。",
            "按服务端日界线（DB时区）判定每日一次，奖励值从配置读取。",
            [
                "刷新时区口径（服务端/用户端）",
                "打卡奖励是否允许活动期调整（是/否）",
                "若允许调整，是否回溯历史记录（是/否）",
            ],
            "先按服务端时区 + 固定配置实现，避免规则漂移。",
            options=[
                "方案A（推荐）：服务端时区 + 不回溯",
                "方案B：用户时区 + 回溯（实现复杂度高）",
            ],
        )

    if "F004" in feature_map:
        add_item(
            blocker_items,
            "F004 用户反馈链路",
            "状态流转与回写展示口径",
            "意见反馈处理后状态集合、回复展示范围、批量处理失败回执格式是否冻结？",
            "影响后台处理流程、小程序“我的反馈”显示一致性、运营复核效率。",
            "默认状态流：`PENDING -> PROCESSED`；`is_adopted` 必填；`reply` 可空但长度 <= 100；处理后实时回写小程序。",
            [
                "是否需要额外状态（驳回/撤销）（是/否）",
                "小程序是否展示管理员回复全文（是/否）",
                "批量处理失败是否导出错误明细（是/否）",
            ],
            "按单向状态流先开发，扩展状态放后续版本。",
            options=[
                "方案A（推荐）：PENDING -> PROCESSED 单向状态流",
                "方案B：增加驳回/撤销状态（需扩展状态机）",
            ],
        )

    if "F001" in feature_map:
        add_item(
            non_blocker_items,
            "F001 下载限制",
            "购买会员承接文案与跳转一致性",
            "购买会员按钮文案是否统一为“购买会员”？是否需要 AB 文案实验？",
            "影响转化漏斗统计和前端埋点维度，但不阻塞核心能力开发。",
            "统一“购买会员”，跳转复用已上线链路（需求6）。",
            ["按钮文案最终值", "是否开启 AB 文案实验（是/否）"],
            "按统一文案先上线，后续通过配置或实验平台优化。",
            owner="产品经理/运营",
            due="上线前确认",
            options=[
                "方案A（推荐）：统一“购买会员”文案",
                "方案B：AB文案实验（需埋点与实验平台支持）",
            ],
        )

    if "F003" in feature_map:
        add_item(
            non_blocker_items,
            "F003 学习打卡",
            "打卡页提示语运营化策略",
            "底部固定提示语后续是否允许按活动配置化？",
            "影响运营活动灵活度，不影响主流程开发。",
            "当前固定文案为“打卡任务可获得积分规则”，后续可新增配置能力。",
            ["是否允许配置化（是/否）", "若允许，配置生效范围（全量/分渠道/分活动）"],
            "先按固定文案实现，配置化能力后续迭代。",
            owner="产品经理/运营",
            due="上线前确认",
            options=[
                "方案A（推荐）：固定文案",
                "方案B：配置化文案（后续迭代）",
            ],
        )

    supplemental_items = [item.strip() for item in confirmation_items if item.strip()]
    if supplemental_items:
        merged_summary = "；".join(supplemental_items[:3])
        add_item(
            non_blocker_items,
            "PRD补充",
            "PRD补充风险收口",
            f"以下补充风险是否按默认策略执行：{merged_summary}",
            "影响提测前口径冻结与测试覆盖边界，但不阻塞主流程编码。",
            "按现网兼容 + 配置优先策略落地，差异项通过配置和联调补齐。",
            [
                f"是否同意按默认策略收口（范围：{merged_summary}）（是/否）",
                "若不同意，请给出替代口径与生效时间",
            ],
            "按默认策略推进开发，产品评审会后统一回填最终结论。",
            owner="产品经理",
            due="评审会当日",
            options=[
                "方案A（推荐）：按默认策略收口",
                "方案B：补充替代口径并二次评审",
            ],
        )

    if not blocker_items and feature_specs:
        for spec in feature_specs[:4]:
            first_rule = (spec.rules or spec.target or [spec.title])[0]
            add_item(
                blocker_items,
                f"{spec.code} {spec.title}",
                "规则口径冻结",
                first_rule,
                "影响对应功能实现与验收标准。",
                "按 PRD 当前描述执行。",
                ["产品最终结论", "是否阻塞开发（是/否）"],
                "先按 PRD 文本口径执行。",
            )

    blocker_detail_lines: list[str] = []
    non_blocker_detail_lines: list[str] = []
    review_script_rows = [
        "| 编号 | 评审提问（逐字可读） | 推荐结论 | 选择备选方案时的额外动作 | 需同步回填的文档 |",
        "|---|---|---|---|---|",
    ]
    sync_action_rows = [
        "| 编号 | 决策后同步动作 | 责任人 | 截止时间 | 验收方式 |",
        "|---|---|---|---|---|",
    ]
    result_rows = [
        "| 编号 | 级别 | 关联功能 | 确认项 | 建议默认口径 | 产品结论 | 确认人 | 截止日期 | 是否阻塞 |",
        "|---|---|---|---|---|---|---|---|---|",
    ]

    for index, item in enumerate(blocker_items, start=1):
        label = f"A{index}"
        blocker_detail_lines.extend(
            [
                f"### {label} {item['title']}",
                "",
                f"- 关联功能：{item['feature']}",
                f"- 决策问题：{item['question']}",
                f"- 影响范围：{item['impact']}",
                f"- 建议默认口径：{item['default_decision']}",
                f"- 决策负责人：{item['owner']}",
                f"- 决策截止：{item['due']}",
                "- 可选方案：",
                *[f"  - {option}" for option in item["options"]],
                "- 必填决策字段：",
                *[f"  - {decision}" for decision in item["decisions"]],
                f"- 未确认前开发策略：{item['fallback_action']}",
                "",
            ]
        )
        result_rows.append(
            f"| {label} | 阻塞级 | {item['feature']} | {item['title']} | {item['default_decision']} | 请选择（方案A/方案B） | {item['owner']} | {item['due']} | 是 |"
        )
        review_script_rows.append(
            f"| {label} | {item['question']} | 方案A（推荐） | 若选择方案B，需补充完整规则清单并二次评审后再冻结 | PRD重评估版、任务看板、流程图与实现对齐、UAT用例 |"
        )
        sync_action_rows.append(
            f"| {label} | 1) 回填产品结论；2) 更新任务验收口径；3) 更新测试场景与失败分支 | {item['owner']} | {item['due']} | 交叉核对 PRD 追溯矩阵 + 任务看板验收列 |"
        )

    for index, item in enumerate(non_blocker_items[:3], start=1):
        label = f"B{index}"
        non_blocker_detail_lines.extend(
            [
                f"### {label} {item['title']}",
                "",
                f"- 关联功能：{item['feature']}",
                f"- 决策问题：{item['question']}",
                f"- 影响范围：{item['impact']}",
                f"- 建议默认口径：{item['default_decision']}",
                f"- 决策负责人：{item['owner']}",
                f"- 决策截止：{item['due']}",
                "- 可选方案：",
                *[f"  - {option}" for option in item["options"]],
                "- 必填决策字段：",
                *[f"  - {decision}" for decision in item["decisions"]],
                f"- 未确认前开发策略：{item['fallback_action']}",
                "",
            ]
        )
        result_rows.append(
            f"| {label} | 非阻塞 | {item['feature']} | {item['title']} | {item['default_decision']} | 请选择（方案A/方案B） | {item['owner']} | {item['due']} | 否 |"
        )
        review_script_rows.append(
            f"| {label} | {item['question']} | 方案A（推荐） | 若选择方案B，需补充配置项/埋点项并确认发布时间窗口 | PRD重评估版、任务看板备注、测试结果文档 |"
        )
        sync_action_rows.append(
            f"| {label} | 1) 记录默认策略或替代策略；2) 补齐配置/埋点计划；3) 评估是否影响上线窗口 | {item['owner']} | {item['due']} | 在产品确认清单与任务看板中可追溯 |"
        )

    return f"""# {doc_date} 产品确认清单 - {theme}

## 文档信息

| 项目 | 内容 |
|------|------|
| 需求主题 | {theme} |
| 对应PRD | `{prd_rel}` |
| 创建日期 | {doc_date} |
| 用途 | 产品/开发/测试在开工前冻结口径，减少返工 |

---

## 使用说明

- 本清单只保留“会影响开发实现或验收结果”的确认项。
- 每项确认必须落到可执行结论：`采用方案 + 生效时间 + 负责人`。
- 未确认项按“建议默认口径”执行，但提测前必须补齐产品签字结论。

---

## A. 阻塞级确认项（需优先冻结）

{chr(10).join(blocker_detail_lines) if blocker_detail_lines else "暂无阻塞级确认项。"}

---

## B. 非阻塞确认项（可并行补齐）

{chr(10).join(non_blocker_detail_lines) if non_blocker_detail_lines else "暂无非阻塞确认项。"}

---

## C. 确认结果记录表

{chr(10).join(result_rows)}

---

## D. 评审会议脚本（可直接逐条确认）

{chr(10).join(review_script_rows)}

---

## E. 决策后同步动作清单（防止只确认不落地）

{chr(10).join(sync_action_rows)}

---

## F. 评审结论

- [ ] 阻塞级确认项已冻结，可进入全面开发。
- [ ] 非阻塞确认项已记录默认策略，可并行推进。
- [ ] 确认结果已同步回 PRD、任务看板、测试口径。
"""


def build_impl_alignment(
    doc_date: str,
    theme: str,
    prd_rel: str,
    top_modules: list[str],
    feature_specs: list[FeatureSpec],
    blueprints: dict[str, FeatureBlueprint],
) -> str:
    def extract_table_names(lines: list[str]) -> str:
        names: list[str] = []
        for line in lines:
            candidates = BACKTICK_RE.findall(line)
            if not candidates:
                continue
            token = candidates[0].strip().lower()
            if not token:
                continue
            if " " in token:
                continue
            if any(ch in token for ch in ("(", ")", "{", "}", ".", ",")):
                continue
            if token.startswith(("idx_", "uk_", "pk_")):
                continue
            names.append(token)
        return "、".join(uniq(names)[:3]) or "待补充"

    rows = [
        "| 功能 | 关键入口接口 | 事务边界 | 关键错误码 | 核心落库表 | 代码落点 |",
        "|---|---|---|---|---|---|",
    ]
    failure_rows = [
        "| 功能 | 失败触发点 | 返回错误码 | 回滚/补偿动作 | 可观测信号（日志/表） |",
        "|---|---|---|---|---|",
    ]
    spec_iter = feature_specs[:10] if feature_specs else []
    for spec in spec_iter:
        blueprint = blueprints.get(spec.code)
        if blueprint:
            entry = "；".join(blueprint.planned_apis[:2]) or "待补充"
            txn = "；".join((blueprint.concurrency_controls + blueprint.rollback_strategy)[:2]) or "待补充"
            errors = "、".join(code.replace("`", "") for code in blueprint.error_codes[:3]) or "待补充"
            tables = extract_table_names(blueprint.table_design)
            touch = "；".join(blueprint.code_touchpoints[:2]) or "待补充"
            failure_rows.append(
                f"| {spec.code} {spec.title} | {entry} | {errors} | {'；'.join(blueprint.rollback_strategy[:2]) or '失败返回错误码并保持数据不变'} | 应用日志(traceId) + 关键业务表核对 |"
            )
        else:
            entry = "；".join((spec.rules or spec.target)[:2]) or spec.title
            txn = "待补充"
            errors = "待补充"
            tables = "待补充"
            touch = "待补充"
            failure_rows.append(
                f"| {spec.code} {spec.title} | 待补充 | 待补充 | 待补充 | 待补充 |"
            )
        rows.append(f"| {spec.code} {spec.title} | {entry} | {txn} | {errors} | {tables} | {touch} |")

    if len(rows) == 2:
        for module in top_modules[:6] or ["核心模块"]:
            rows.append(f"| {module} | 待补充 | 待补充 | 待补充 | 待补充 | 待补充 |")
            failure_rows.append(f"| {module} | 待补充 | 待补充 | 待补充 | 待补充 |")

    diagram_map = {
        "F001": """```mermaid
sequenceDiagram
    participant Mini as 小程序试卷详情
    participant PC as PaperController
    participant RS as PaperDownloadRightsService
    participant FC as FileController
    participant DB as MySQL

    Mini->>PC: GET /api/papers/{id}/download-rights
    PC->>RS: getRights(userId,paperId)
    RS->>DB: 统计手机号维度总额度/日额度
    DB-->>RS: remainingTotal/remainingDaily
    RS-->>PC: canDownload + membershipTip + limitRule
    PC-->>Mini: 权益信息

    Mini->>FC: GET /api/file/token?paperId=
    FC->>RS: 再次校验权益快照
    alt 权益不足
        RS-->>FC: reject(DL-001/DL-002/DL-003)
        FC-->>Mini: 业务错误码 + 提示文案 + traceId
    else 权益可用
        RS-->>FC: allow
        FC-->>Mini: token + remaining + ruleVersion
    end

    Mini->>FC: GET /api/file/pdfs/{token}?action=download
    FC->>DB: 事务: papers.download_count +1 & insert paper_download_records
    alt token失效/写库失败
        DB-->>FC: rollback
        FC-->>Mini: DL-004 + traceId
    else 下载成功
        DB-->>FC: commit
        FC-->>Mini: 文件流
    end
```""",
        "F002": """```mermaid
sequenceDiagram
    participant Mini as 小程序积分页
    participant PTC as PointsController
    participant PRS as PointsRedemptionService
    participant MCS as MemberCardActivationService
    participant DB as MySQL

    Mini->>PTC: GET /api/points/me
    PTC->>DB: 查询/初始化积分账户
    DB-->>PTC: totalPoints + maxRedeemDays
    PTC-->>Mini: 账户信息

    Mini->>PTC: POST /api/points/redeem(days)
    PTC->>PRS: redeem(userId,days)
    alt 积分不足/参数非法
        PRS-->>PTC: PT-001/PT-002
        PTC-->>Mini: 错误码 + 规则提示
    else 参数合法
        PRS->>DB: 事务: 扣积分 -> 写流水 -> 写兑换记录
        PRS->>MCS: createPendingMemberCard()
        alt 会员卡创建失败
            MCS-->>PRS: fail
            PRS->>DB: rollback
            PRS-->>PTC: PT-004
            PTC-->>Mini: 兑换失败(已回滚)
        else 创建成功
            MCS-->>PRS: memberCardId
            PRS->>DB: commit
            PRS-->>PTC: redeem success
            PTC-->>Mini: remainingPoints + memberCardId
        end
    end
```""",
        "F003": """```mermaid
sequenceDiagram
    participant Mini as 小程序打卡页
    participant SCC as StudyCheckinController
    participant SCS as StudyCheckinService
    participant PLS as PointsLedgerService
    participant DB as MySQL

    Mini->>SCC: GET /api/checkins/calendar
    SCC->>SCS: getCalendar(userId,year,month)
    SCS->>DB: 查询月历与统计
    DB-->>SCS: monthCount/totalCount
    SCS-->>SCC: calendar + summary
    SCC-->>Mini: 展示打卡信息

    Mini->>SCC: POST /api/checkins
    SCC->>SCS: checkinToday(userId)
    alt 已打卡
        SCS-->>SCC: CK-001
        SCC-->>Mini: 今日已打卡，请勿重复提交
    else 未打卡
        SCS->>DB: insert study_checkins(uk_user_date)
        SCS->>PLS: appendDetail(checkin_reward)
        alt 积分写入失败
            PLS-->>SCS: fail
            SCS->>DB: rollback
            SCS-->>SCC: CK-003
            SCC-->>Mini: 打卡失败，请重试
        else 成功
            PLS->>DB: insert user_points_details
            DB-->>SCS: commit
            SCS-->>SCC: checkin success
            SCC-->>Mini: 今日已打卡 + 最新统计
        end
    end
```""",
        "F004": """```mermaid
sequenceDiagram
    participant Mini as 小程序反馈提交页
    participant Admin as 后台反馈管理页
    participant UFC as UserFeedbackController
    participant UFS as UserFeedbackService
    participant DB as MySQL

    Mini->>UFC: POST /api/user-feedbacks
    UFC->>UFS: submit(feedbackType,content,images)
    UFS->>DB: insert user_feedbacks(status=PENDING)
    DB-->>UFS: feedbackId
    UFS-->>Mini: submit success

    Admin->>UFC: PUT /api/user-feedbacks/{id}/process
    UFC->>UFS: process(adopted,reply<=100)
    alt 参数非法/记录不存在
        UFS-->>UFC: FB-003/FB-004/FB-005
        UFC-->>Admin: 错误码 + 修正提示
    else 参数合法
        UFS->>DB: update status/is_adopted/reply/processed_at
        DB-->>UFS: commit
        UFS-->>Admin: process success
    end
```""",
    }

    detail_sections: list[str] = []
    for spec in spec_iter:
        blueprint = blueprints.get(spec.code)
        if not blueprint:
            detail_sections.extend(
                [
                    f"### {spec.code} {spec.title}",
                    "",
                    "- 当前仅有 PRD 描述，需在编码前补齐接口、事务、错误码与落库明细。",
                    "",
                ]
            )
            continue

        detail_sections.extend(
            [
                f"### {spec.code} {spec.title}",
                "",
                "**主流程步骤**",
                "",
                *[f"{idx}. {step}" for idx, step in enumerate(blueprint.sequence_steps, start=1)],
                "",
                "**接口与契约**",
                "",
                *[f"- {line}" for line in (blueprint.planned_apis + blueprint.api_contracts)[:6]],
                "",
                "**事务与一致性**",
                "",
                *[f"- {line}" for line in (blueprint.concurrency_controls + blueprint.rollback_strategy)[:6]],
                "",
                "**异常码与失败处理**",
                "",
                *[f"- {line}" for line in blueprint.error_codes[:6]],
                "",
                "**代码落点**",
                "",
                *[f"- {line}" for line in blueprint.code_touchpoints[:6]],
                "",
            ]
        )
        if spec.code in diagram_map:
            detail_sections.extend(
                [
                    "**详细时序图**",
                    "",
                    diagram_map[spec.code],
                    "",
                ]
            )

    return f"""# {doc_date} 流程图与实现对齐 - {theme}

## 1. 文档目标

- 将 PRD 规则映射为可编码的接口、事务、异常和落库步骤。
- 明确每个功能点的入口、校验、状态变化、失败分支和回滚策略。
- 为开发/测试提供可直接执行的流程基线。

## 2. 跨模块总流程

```mermaid
flowchart TD
    A[入口层发起请求/任务] --> B[参数校验 + 鉴权]
    B --> C{{功能分支}}
    C -->|能力A| D1[规则校验与资源准备]
    C -->|能力B| D2[状态计算与事务处理]
    C -->|能力C| D3[幂等校验与结果落库]
    C -->|能力D| D4[异步任务与回写处理]
    D1 --> E[Service执行业务规则]
    D2 --> E
    D3 --> E
    D4 --> E
    E --> F{{事务提交}}
    F -->|成功| G[返回响应并刷新前端态]
    F -->|失败| H[回滚并返回业务错误码]
```

## 3. 需求-实现对齐矩阵

{chr(10).join(rows)}

## 3.1 失败分支与回滚矩阵（开发自检与联调必查）

{chr(10).join(failure_rows)}

## 4. 分功能详细对齐

{chr(10).join(detail_sections) if detail_sections else "待补充详细流程。"}
"""


def build_acceptance_doc(doc_date: str, theme: str, use_cases: list[str]) -> str:
    rows = ["| 用例ID | 场景 | 前置数据 | 执行步骤 | 预期结果 | 实际结果 | 证据 | 状态 |", "| --- | --- | --- | --- | --- | --- | --- | --- |"]
    cases = use_cases[:8] or ["核心流程可用性", "事务一致性", "幂等与防重", "异常分支回滚"]
    for index, case in enumerate(cases, start=1):
        rows.append(
            f"| IT-{index:02d} | {case} | 准备与场景匹配的输入数据与配置项 | 按接口/流程主链路执行并覆盖异常分支 | 返回结果、落库结果、展示结果三者一致 | 待执行 | 请求日志/SQL核对/截图 | 待执行 |"
        )
    return f"""# {doc_date} 联调验收记录 - {theme}

## 1. 目标

- 按 PRD 验证关键业务场景、异常处理逻辑和联调结果。

## 2. 环境信息

- 环境：`dev`（功能联调）/`staging`（提测回归）
- 数据准备：按 UAT 用例准备输入样本、配置项与权限上下文

## 3. 执行步骤

1. 按测试用例准备业务数据、配置和前置条件。
2. 执行对应业务流程或页面操作。
3. 核对接口结果、数据落库、页面表现和异常提示。

## 4. 结果记录

{chr(10).join(rows)}

## 5. 结论

- 联调通过标准：主流程通过 + 关键异常码可复现 + 关键表落库一致。
"""


def build_test_result_doc(doc_date: str, theme: str, use_cases: list[str]) -> str:
    rows = ["| 检查项 | 验证方式 | 命令/步骤 | 预期 | 实际 | 证据 | 状态 |", "| --- | --- | --- | --- | --- | --- | --- |"]
    cases = use_cases[:8] or ["接口构建检查", "关键SQL脚本检查", "前端页面主流程检查", "异常分支回归检查"]
    for case in cases:
        rows.append(f"| {case} | 自动化+手工 | 按任务拆解执行对应测试步骤 | 结果符合 PRD 与设计口径 | 待执行 | 日志/截图/SQL结果 | 待执行 |")
    return f"""# {doc_date} 测试结果 - {theme}

## 1. 目标

- 为后续自动化测试和联调回写预置测试范围。

## 2. 环境信息

- 环境：`dev` / `staging`
- 数据准备：参考 UAT 用例与联调记录，确保可重复执行

## 3. 执行步骤

1. 执行构建或编译校验。
2. 执行关键自动化测试或数据校验脚本。
3. 对阻塞场景记录替代验证和剩余待验证点。

## 4. 结果记录

{chr(10).join(rows)}

## 5. 结论

- 提测前最小门槛：核心功能点至少 1 条主流程通过记录 + 1 条异常分支通过记录。
"""


def build_uat_cases(doc_date: str, theme: str, use_cases: list[str]) -> str:
    rows = ["| 用例ID | 优先级 | 场景 | 前置条件 | 执行步骤 | 期望结果 | 结果记录 |", "| --- | --- | --- | --- | --- | --- | --- |"]
    cases = use_cases[:10] or ["核心能力验证", "数据一致性验证", "异常分支验证", "端到端流程验证"]
    for index, case in enumerate(cases, start=1):
        rows.append(
            f"| UAT-{index:02d} | {'P0' if index <= 4 else 'P1'} | {case} | 准备与场景匹配的用户、配置、权限与样本数据 | 执行业务流程并核对接口、页面、数据和异常提示 | 结果符合 PRD 且可追溯到代码与数据 | 待执行 |"
        )
    return f"""# {doc_date} UAT测试用例 - {theme}

## 1. 目标

- 用于产品、测试和业务在开发完成后执行场景验收。

## 2. 环境信息

- 环境：`staging`（默认）
- 版本：提测版本号待回填

## 3. 执行步骤

1. 按用例准备基础数据和配置。
2. 执行对应业务动作或页面操作。
3. 核对接口结果、数据落库、异常提示和页面输出。

## 4. 结果记录

{chr(10).join(rows)}

## 5. 结论

- 用例已覆盖核心功能与关键异常场景，执行后需同步回填测试结果与追溯文档。
"""


def build_sql_ddl(doc_date: str, theme: str) -> str:
    return f"""-- {doc_date} DDL - {theme}
-- 目的：提供通用需求的数据模型骨架（按项目实际替换）
-- 执行环境：MySQL 8.x

START TRANSACTION;

CREATE TABLE IF NOT EXISTS `domain_entity_main` (
  `id` BIGINT NOT NULL AUTO_INCREMENT,
  `entity_key` VARCHAR(128) NOT NULL,
  `entity_type` VARCHAR(64) NOT NULL,
  `status` VARCHAR(32) NOT NULL DEFAULT 'ACTIVE',
  `payload` JSON DEFAULT NULL,
  `created_at` DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  `updated_at` DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6) ON UPDATE CURRENT_TIMESTAMP(6),
  PRIMARY KEY (`id`),
  UNIQUE KEY `uk_dem_entity_key` (`entity_key`),
  KEY `idx_dem_type_status` (`entity_type`, `status`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci COMMENT='通用主实体表';

CREATE TABLE IF NOT EXISTS `domain_entity_event` (
  `event_id` BIGINT NOT NULL AUTO_INCREMENT,
  `entity_id` BIGINT NOT NULL,
  `event_type` VARCHAR(64) NOT NULL,
  `event_source` VARCHAR(128) DEFAULT NULL,
  `event_payload` JSON DEFAULT NULL,
  `occurred_at` DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6),
  PRIMARY KEY (`event_id`),
  KEY `idx_dee_entity_time` (`entity_id`, `occurred_at`),
  KEY `idx_dee_type_time` (`event_type`, `occurred_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci COMMENT='通用事件表';

COMMIT;
"""


def build_sql_ddl_field_fix(doc_date: str, theme: str) -> str:
    return f"""-- {doc_date} DDL-字段修正 - {theme}
-- 目的：对现有表做通用向前兼容字段补充

ALTER TABLE `domain_entity_main`
  ADD COLUMN IF NOT EXISTS `biz_owner` VARCHAR(128) DEFAULT NULL COMMENT '业务归属' AFTER `entity_type`,
  ADD COLUMN IF NOT EXISTS `remark` VARCHAR(500) DEFAULT NULL COMMENT '备注' AFTER `payload`;

ALTER TABLE `domain_entity_event`
  ADD COLUMN IF NOT EXISTS `trace_id` VARCHAR(128) DEFAULT NULL COMMENT '链路追踪ID' AFTER `event_source`;

-- 配置项建议（如不存在时补充）
INSERT INTO `configs` (`config_key`, `config_value`, `config_description`, `updated_at`)
SELECT 'domain_default_rule', '{{"enabled":true}}', '通用规则配置', NOW(6)
WHERE NOT EXISTS (
  SELECT 1 FROM `configs` WHERE `config_key` = 'domain_default_rule'
);
"""


def build_sql_ddl_index_fix(doc_date: str, theme: str) -> str:
    return f"""-- {doc_date} DDL-索引修正 - {theme}
-- 目的：补齐高频查询索引

CREATE INDEX IF NOT EXISTS `idx_dem_type_status`
  ON `domain_entity_main` (`entity_type`, `status`);

CREATE INDEX IF NOT EXISTS `idx_dem_updated_at`
  ON `domain_entity_main` (`updated_at`);

CREATE INDEX IF NOT EXISTS `idx_dee_entity_time`
  ON `domain_entity_event` (`entity_id`, `occurred_at`);

CREATE INDEX IF NOT EXISTS `idx_dee_type_time`
  ON `domain_entity_event` (`event_type`, `occurred_at`);
"""


def build_sql_ddl_slim_fields(doc_date: str, theme: str) -> str:
    return f"""-- {doc_date} DDL-精简字段 - {theme}
-- 目的：记录字段收敛建议（默认不直接删除）
-- 建议：评审确认后再执行 DROP/迁移

-- 1. 若某些 JSON 字段确认长期无使用，可评估降级为普通字符串字段
-- ALTER TABLE `domain_entity_main` MODIFY COLUMN `payload` VARCHAR(4096);

-- 2. 若备注字段无业务使用，可评估收敛
-- ALTER TABLE `domain_entity_main` DROP COLUMN `remark`;

-- 3. 执行前建议先核对数据量
SELECT COUNT(*) AS entity_total FROM `domain_entity_main`;
SELECT COUNT(*) AS event_total FROM `domain_entity_event`;
"""


def build_sql_ddl_sequence(doc_date: str, theme: str) -> str:
    return f"""-- {doc_date} DDL-主键序列 - {theme}
-- 目的：统一主键策略检查（MySQL 通常使用 AUTO_INCREMENT）

SHOW CREATE TABLE `domain_entity_main`;
SHOW CREATE TABLE `domain_entity_event`;

-- 示例：如需重置序列请在业务低峰期执行
-- ALTER TABLE `domain_entity_main` AUTO_INCREMENT = 100000;
"""


def build_sql_fix_history(doc_date: str, theme: str) -> str:
    return f"""-- {doc_date} SQL-历史补全 - {theme}
-- 目的：为历史实体补齐通用主数据和事件基线（按需执行）

START TRANSACTION;

INSERT INTO `domain_entity_main` (`entity_key`, `entity_type`, `status`, `payload`, `created_at`, `updated_at`)
SELECT CONCAT('HIS-', src.`id`), 'legacy', 'ACTIVE', JSON_OBJECT('source', 'history-import'), NOW(6), NOW(6)
FROM `legacy_source` src
LEFT JOIN `domain_entity_main` dem ON dem.`entity_key` = CONCAT('HIS-', src.`id`)
WHERE dem.`id` IS NULL
LIMIT 200;

COMMIT;
"""


def build_sql_fix_manual(doc_date: str, theme: str) -> str:
    return f"""-- {doc_date} SQL-人工映射模板 - {theme}
-- 目的：给人工映射和临时修复留标准入口
-- 使用：将 TODO 替换为真实值后执行

INSERT INTO `domain_entity_main` (`entity_key`, `entity_type`, `status`, `payload`, `created_at`, `updated_at`)
VALUES ('TODO_ENTITY_KEY', 'TODO_TYPE', 'ACTIVE', JSON_OBJECT('source', 'manual_fix'), NOW(6), NOW(6));

INSERT INTO `domain_entity_event` (`entity_id`, `event_type`, `event_source`, `event_payload`, `occurred_at`)
VALUES (/* TODO_ENTITY_ID */ 0, 'TODO_EVENT_TYPE', 'manual_fix', JSON_OBJECT('remark', 'manual fix'), NOW(6));
"""


def build_sql_testdata(doc_date: str, theme: str) -> str:
    return f"""-- {doc_date} SQL-测试样本 - {theme}
-- 目的：构造联调最小样本数据

START TRANSACTION;

INSERT INTO `domain_entity_main` (`entity_key`, `entity_type`, `status`, `payload`, `created_at`, `updated_at`)
VALUES ('TEST-ENTITY-001', 'sample', 'ACTIVE', JSON_OBJECT('name', '样本实体'), NOW(6), NOW(6));

INSERT INTO `domain_entity_event` (`entity_id`, `event_type`, `event_source`, `event_payload`, `occurred_at`)
VALUES (1, 'created', 'testdata', JSON_OBJECT('remark', 'test event'), NOW(6));

COMMIT;
"""


def generated_tasks(
    initial_task_id: str,
    task_file: Path,
    sync_date: str,
    breakdown_link: str,
    function_items: list[FunctionItem],
    top_modules: list[str],
    child_map: dict[str, list[str]],
    use_cases: list[str],
    blueprints: dict[str, FeatureBlueprint],
) -> list[tuple[str, GeneratedTask]]:
    def compact_join(items: list[str], fallback: str, limit: int) -> str:
        values = [segment.strip().rstrip("；。") for segment in items if segment and segment.strip()]
        picked = values[:limit]
        if not picked:
            return fallback
        return "；".join(picked)

    feature_items = function_items or [
        FunctionItem(code=f"T{index:03d}", module=module, name=f"{module}实现", description=module, priority="P0", detail_points=child_map.get(module, []), acceptance_points=[])
        for index, module in enumerate((top_modules[:4] or ["数据模型与配置", "核心处理能力", "入口与交互能力"]), start=1)
    ]
    tasks: list[GeneratedTask] = []
    for index, item in enumerate(feature_items, start=1):
        scope_seed = compact_join(item.detail_points, item.description, 3)
        blueprint = blueprints.get(item.code)
        acceptance_seed = (
            compact_join(blueprint.acceptance_steps, f"{item.name} 相关能力可按 PRD 验证", 3)
            if blueprint and blueprint.acceptance_steps
            else (compact_join(item.acceptance_points, use_cases[index - 1] if index - 1 < len(use_cases) else f"{item.name} 相关能力可按 PRD 验证", 2))
        )
        api_seed = (
            compact_join(blueprint.planned_apis, "接口按详细开发设计实现", 2)
            if blueprint and blueprint.planned_apis
            else "接口按详细开发设计实现"
        )
        code_seed = (
            compact_join(blueprint.code_touchpoints, "代码落点按开发任务拆解执行", 2)
            if blueprint and blueprint.code_touchpoints
            else "代码落点按开发任务拆解执行"
        )
        error_seed = (
            compact_join(blueprint.error_codes, "错误码按模块统一定义", 2)
            if blueprint and blueprint.error_codes
            else "错误码按模块统一定义"
        )
        consistency_seed = (
            compact_join(blueprint.concurrency_controls, "并发与一致性按详细开发设计实现", 2)
            if blueprint and blueprint.concurrency_controls
            else "并发与一致性按详细开发设计实现"
        )
        acceptance_text = (
            f"范围：{scope_seed} ｜ "
            f"优先级：{item.priority} ｜ "
            f"接口：{api_seed} ｜ "
            f"代码：{code_seed} ｜ "
            f"错误码：{error_seed} ｜ "
            f"一致性：{consistency_seed} ｜ "
            f"验收：{acceptance_seed}"
        )
        tasks.append(
            GeneratedTask(
                title=f"{item.code} {item.name}",
                acceptance=acceptance_text,
                doc_link=breakdown_link,
            )
        )

    tasks.append(
        GeneratedTask(
            title="联调验证与发布闸门准备",
            acceptance="关键业务场景、异常阻塞和替代验证均已记录，可进入发布闸门检查",
            doc_link=breakdown_link,
        )
    )

    results: list[tuple[str, GeneratedTask]] = []
    first_id = initial_task_id
    results.append((first_id, tasks[0]))

    id_match = re.match(r"^(.*?)(\d+)$", initial_task_id)
    if id_match:
        prefix = id_match.group(1)
        width = len(id_match.group(2))
        start = int(id_match.group(2))
        for offset, task in enumerate(tasks[1:], start=1):
            results.append((f"{prefix}{start + offset:0{width}d}", task))
        return results

    generated_seed = next_task_id(task_file, sync_date)
    seed_match = re.match(r"^(.*?)(\d+)$", generated_seed)
    if not seed_match:
        return results
    prefix = seed_match.group(1)
    width = len(seed_match.group(2))
    start = int(seed_match.group(2))
    for offset, task in enumerate(tasks[1:]):
        results.append((f"{prefix}{start + offset:0{width}d}", task))
    return results


def current_task_content(task_file: Path, task_id: str) -> tuple[str | None, str | None]:
    if not task_file.exists():
        return None, None
    row = find_task_row(task_file, task_id)
    if row is None:
        return None, None
    title = get_cell(row.cells, row.header_map, ("任务标题", "title"))
    acceptance = get_cell(row.cells, row.header_map, ("验收标准", "acceptance"))
    return title, acceptance


def next_task_id(task_file: Path, doc_date: str) -> str:
    prefix = f"TASK-{doc_date}-"
    if not task_file.exists():
        return f"{prefix}01"
    content = task_file.read_text(encoding="utf-8")
    nums = [int(match.group(1)) for match in re.finditer(rf"{re.escape(prefix)}(\d+)", content)]
    return f"{prefix}{max(nums, default=0) + 1:02d}"


def derive_legacy_req_id(req_id: str, doc_date: str) -> str | None:
    match = REQ_NEW_ID_RE.match(req_id.strip())
    if not match:
        return None
    return f"REQ-{doc_date}-{int(match.group(2)):02d}"


def derive_legacy_task_ids(task_id: str, doc_date: str, count: int) -> list[str]:
    if count <= 0:
        return []
    match = re.match(r"^TASK-(\d{8})-(\d+)$", task_id.strip())
    if not match:
        return []
    start = int(match.group(2))
    return [f"TASK-{doc_date}-{start + offset:02d}" for offset in range(count)]


def purge_legacy_task_board_rows(
    task_file: Path,
    legacy_req_id: str | None,
    legacy_task_ids: list[str],
    dry_run: bool,
) -> None:
    if not task_file.exists():
        return

    original_lines = task_file.read_text(encoding="utf-8").splitlines()
    lines = list(original_lines)

    if legacy_req_id:
        heading = f"## {legacy_req_id} "
        start = None
        end = None
        for idx, line in enumerate(lines):
            if line.startswith(heading):
                start = idx
                end = len(lines)
                for j in range(idx + 1, len(lines)):
                    if lines[j].startswith("## "):
                        end = j
                        break
                break
        if start is not None and end is not None:
            del lines[start:end]
            while len(lines) >= 2 and not lines[-1].strip() and not lines[-2].strip():
                lines.pop()

    legacy_task_set = set(legacy_task_ids)
    if legacy_task_set:
        filtered: list[str] = []
        for line in lines:
            should_drop = False
            if line.strip().startswith("|"):
                for task in legacy_task_set:
                    if f"`{task}`" in line:
                        should_drop = True
                        break
            if not should_drop:
                filtered.append(line)
        lines = filtered

    if lines == original_lines:
        return
    if dry_run:
        print("- cleanup-legacy-task-board: would remove legacy req/task rows")
        return
    task_file.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("- cleanup-legacy-task-board: removed legacy req/task rows")


def purge_legacy_task_index_rows(
    index_file: Path,
    legacy_task_ids: list[str],
    dry_run: bool,
) -> None:
    if not index_file.exists() or not legacy_task_ids:
        return
    original_lines = index_file.read_text(encoding="utf-8").splitlines()
    legacy_task_set = set(legacy_task_ids)
    lines: list[str] = []
    for line in original_lines:
        should_drop = False
        if line.strip().startswith("|"):
            for task in legacy_task_set:
                if f"`{task}`" in line:
                    should_drop = True
                    break
        if not should_drop:
            lines.append(line)
    if lines == original_lines:
        return
    if dry_run:
        print("- cleanup-legacy-task-index: would remove legacy task rows")
        return
    index_file.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8")
    print("- cleanup-legacy-task-index: removed legacy task rows")


def main() -> int:
    parser = argparse.ArgumentParser(description="Populate requirement bundle docs with PRD-driven body content")
    add_profile_arg(parser)
    parser.add_argument("--req-file", help="Requirement pool file")
    parser.add_argument("--task-file", help="Task board file")
    parser.add_argument("--req-id", required=True)
    parser.add_argument("--initial-task-id", required=True)
    parser.add_argument("--theme", required=True)
    parser.add_argument("--date", required=True)
    parser.add_argument("--bundle-dir", required=True, help="Requirement bundle directory")
    parser.add_argument("--prd-file", required=True, help="PRD markdown file")
    parser.add_argument("--preserve-non-placeholder", action="store_true", help="Preserve existing non-placeholder document contents on rerun")
    add_dry_run_arg(parser)
    args = parser.parse_args()

    profile = load_profile_from_args(args)
    project_paths = ProjectPaths.from_profile(profile, Path.cwd())
    section_title_map = get_section_title_map(profile)
    table_column_aliases = get_table_column_aliases(profile)
    acceptance_alias_map, acceptance_items_map, test_case_keyword_map, acceptance_fallback_map = get_feature_rule_maps(profile)
    req_file = Path(args.req_file).resolve() if args.req_file else project_paths.requirements_pool
    task_file = Path(args.task_file).resolve() if args.task_file else project_paths.task_board
    bundle_dir = Path(args.bundle_dir).resolve()
    prd_file = Path(args.prd_file).resolve()

    if not prd_file.exists():
        print(f"Error: PRD file not found: {prd_file}")
        return 1

    design_dir = bundle_dir / "design"
    testing_dir = bundle_dir / "testing"
    if not design_dir.exists():
        print(f"Error: design directory not found: {design_dir}")
        return 1

    tech_design = find_doc(design_dir, "技术设计")
    detailed_design = find_doc(design_dir, "详细开发设计")
    breakdown = find_doc(design_dir, "开发任务拆解")
    physical_design = find_doc(design_dir, "物理表设计")
    table_mapping = find_doc(design_dir, "表名对照表")
    prd_trace = find_doc(design_dir, "PRD追溯")
    product_confirmation = find_doc(design_dir, "产品确认清单")
    impl_alignment = find_doc(design_dir, "流程图与实现对齐")
    acceptance_doc = find_testing_doc(testing_dir, "联调验收记录")
    test_result_doc = find_testing_doc(testing_dir, "测试结果")
    uat_doc = find_testing_doc(testing_dir, "UAT测试用例")
    sql_ddl = find_sql_doc(bundle_dir / "sql" / "ddl", "01-DDL")
    sql_ddl_field_fix = find_sql_doc(bundle_dir / "sql" / "ddl", "02-DDL-字段修正")
    sql_ddl_index_fix = find_sql_doc(bundle_dir / "sql" / "ddl", "03-DDL-索引修正")
    sql_ddl_slim = find_sql_doc(bundle_dir / "sql" / "ddl", "04-DDL-精简字段")
    sql_ddl_sequence = find_sql_doc(bundle_dir / "sql" / "ddl", "05-DDL-主键序列")
    sql_fix_history = find_sql_doc(bundle_dir / "sql" / "fix", "01-SQL-历史补全")
    sql_fix_manual = find_sql_doc(bundle_dir / "sql" / "fix", "02-SQL-人工映射模板")
    sql_testdata = find_sql_doc(bundle_dir / "sql" / "testdata", "01-SQL-测试样本")

    if not tech_design or not detailed_design or not breakdown or not prd_trace:
        print("Error: required bundle docs are missing")
        return 1

    prd_text = prd_file.read_text(encoding="utf-8")
    sections = parse_sections(prd_text)
    background_sections = find_section_by_titles(sections, section_title_map["background"])
    goal_sections = find_section_by_titles(sections, section_title_map["goal"])
    current_sections = find_section_by_titles(sections, section_title_map["current_state"])
    solution_sections = find_section_by_titles(sections, section_title_map["solution"])
    dependency_sections = find_section_by_titles(sections, section_title_map["dependencies"])
    risk_sections = find_section_by_titles(sections, section_title_map["risks"])
    scenario_sections = find_section_by_titles(sections, section_title_map["scenarios"])
    principle_sections = find_section_by_titles(sections, section_title_map["principles"])
    table_sections = find_section_by_titles(sections, section_title_map["tables"])
    function_section = next(iter(find_section_by_titles(sections, section_title_map["function_list"])), None)
    if function_section is None:
        function_section = find_section_by_table_columns(sections, table_column_aliases["function_list"])
    top_modules, child_map = extract_architecture(sections)
    function_items = extract_function_items(sections, function_section, table_column_aliases["function_list"])
    feature_specs = parse_feature_specs(sections)
    function_items = merge_function_items(function_items, feature_specs)
    feature_lookup = build_feature_lookup(feature_specs)
    acceptance_section = next(iter(find_section_by_titles(sections, section_title_map["acceptance"])), None)
    if acceptance_section is None:
        acceptance_section = find_section_by_table_columns(sections, table_column_aliases["acceptance"])
    acceptance_rows = parse_first_table(acceptance_section)
    focus_test_cases = list_items_from_sections(find_section_by_titles(sections, section_title_map["test_cases"]), limit=10)
    for item in function_items:
        item.acceptance_points = resolve_acceptance_points(
            item,
            acceptance_rows,
            focus_test_cases,
            table_column_aliases["acceptance"],
            acceptance_items_map,
            test_case_keyword_map,
            acceptance_alias_map,
            acceptance_fallback_map,
        )
    if not top_modules and function_items:
        top_modules = uniq([item.module for item in function_items])
    use_cases = derive_use_cases(uniq(list_items_from_sections(scenario_sections, limit=8) + focus_test_cases)[:10], feature_specs)
    raw_principles = list_items_from_sections(principle_sections, limit=8)
    objects = extract_object_names(find_section_by_titles(sections, section_title_map["objects"]), limit=10)
    table_specs = parse_table_specs(table_sections or find_section_by_titles(sections, section_title_map["tables"]), prd_text)
    table_names = [spec.table_name for spec in table_specs if spec.table_name][:10]
    if not table_names:
        table_names = [name for name in objects if "_" in name or "表" in name][:8]
    principles = derive_principles(raw_principles, feature_specs, table_specs)
    current_state_points = derive_current_state(current_sections, feature_specs, table_specs)
    system_change_points = derive_system_change_points(dependency_sections, feature_specs, table_specs)
    confirmation_items = summary_points(risk_sections + find_sections(sections, ("特殊", "不包含", "已确认事项")), limit=5)
    feature_blueprints = build_feature_blueprints(project_paths.workspace_root)

    bundle_rel = bundle_dir.relative_to(project_paths.workspace_root).as_posix()
    design_rel = tech_design.relative_to(project_paths.workspace_root).as_posix()
    detailed_design_rel = detailed_design.relative_to(project_paths.workspace_root).as_posix()
    breakdown_rel = breakdown.relative_to(project_paths.workspace_root).as_posix()
    prd_rel = prd_file.relative_to(project_paths.workspace_root).as_posix()
    uat_rel = uat_doc.relative_to(project_paths.workspace_root).as_posix() if uat_doc else "待补充"

    all_doc_paths = [
        path
        for path in [
            tech_design,
            detailed_design,
            breakdown,
            physical_design,
            table_mapping,
            prd_trace,
            product_confirmation,
            impl_alignment,
            acceptance_doc,
            test_result_doc,
            uat_doc,
        ]
        if path is not None
    ]
    doc_links = [md_link(path, project_paths.workspace_root) for path in all_doc_paths]
    design_doc_links = [md_link(path, project_paths.workspace_root) for path in all_doc_paths if path.parent.name == "design"]

    print_header(
        "Populate Requirement Content",
        {
            "req_id": args.req_id,
            "theme": args.theme,
            "bundle_dir": str(bundle_dir),
            "prd_file": str(prd_file),
            "feature_specs": len(feature_specs),
            "tables": len(table_specs),
            "mode": "dry-run" if args.dry_run else "live",
        },
    )

    write_text(
        tech_design,
        build_tech_design(
            args.date,
            args.theme,
            prd_rel,
            args.req_id,
            background_sections,
            goal_sections,
            current_sections,
            solution_sections,
            dependency_sections,
            risk_sections,
            principles,
            current_state_points,
            system_change_points,
            use_cases,
            top_modules,
            feature_specs,
        ),
        args.dry_run,
        preserve_non_placeholder=args.preserve_non_placeholder,
    )
    write_text(
        detailed_design,
        build_detailed_design(
            args.date,
            args.theme,
            prd_rel,
            top_modules,
            child_map,
            function_items,
            objects,
            table_sections or dependency_sections,
            risk_sections,
            use_cases,
            feature_specs,
            feature_blueprints,
        ),
        args.dry_run,
        preserve_non_placeholder=args.preserve_non_placeholder,
    )
    write_text(
        breakdown,
        build_breakdown(
            args.date,
            args.theme,
            design_rel,
            detailed_design_rel,
            prd_rel,
            function_items,
            top_modules,
            child_map,
            use_cases,
            feature_lookup,
            feature_blueprints,
        ),
        args.dry_run,
        preserve_non_placeholder=args.preserve_non_placeholder,
    )
    if physical_design:
        write_text(
            physical_design,
            build_physical_design(args.date, args.theme, objects, table_names, table_specs),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if table_mapping:
        write_text(
            table_mapping,
            build_table_mapping(args.date, args.theme, objects, table_names, table_specs),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    write_text(
        prd_trace,
        build_prd_trace(
            args.date,
            args.theme,
            prd_rel,
            design_rel,
            detailed_design_rel,
            breakdown_rel,
            uat_rel,
            top_modules,
            child_map,
            feature_specs,
            feature_blueprints,
        ),
        args.dry_run,
        preserve_non_placeholder=args.preserve_non_placeholder,
    )
    if product_confirmation:
        write_text(
            product_confirmation,
            build_product_confirmation(args.date, args.theme, prd_rel, confirmation_items, feature_specs, feature_blueprints),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if impl_alignment:
        write_text(
            impl_alignment,
            build_impl_alignment(args.date, args.theme, prd_rel, top_modules, feature_specs, feature_blueprints),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if acceptance_doc:
        write_text(
            acceptance_doc,
            build_acceptance_doc(args.date, args.theme, use_cases),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if test_result_doc:
        write_text(
            test_result_doc,
            build_test_result_doc(args.date, args.theme, use_cases),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if uat_doc:
        write_text(
            uat_doc,
            build_uat_cases(args.date, args.theme, use_cases),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if sql_ddl:
        write_text(
            sql_ddl,
            build_sql_ddl(args.date, args.theme),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if sql_ddl_field_fix:
        write_text(
            sql_ddl_field_fix,
            build_sql_ddl_field_fix(args.date, args.theme),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if sql_ddl_index_fix:
        write_text(
            sql_ddl_index_fix,
            build_sql_ddl_index_fix(args.date, args.theme),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if sql_ddl_slim:
        write_text(
            sql_ddl_slim,
            build_sql_ddl_slim_fields(args.date, args.theme),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if sql_ddl_sequence:
        write_text(
            sql_ddl_sequence,
            build_sql_ddl_sequence(args.date, args.theme),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if sql_fix_history:
        write_text(
            sql_fix_history,
            build_sql_fix_history(args.date, args.theme),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if sql_fix_manual:
        write_text(
            sql_fix_manual,
            build_sql_fix_manual(args.date, args.theme),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )
    if sql_testdata:
        write_text(
            sql_testdata,
            build_sql_testdata(args.date, args.theme),
            args.dry_run,
            preserve_non_placeholder=args.preserve_non_placeholder,
        )

    background_items = summary_points(background_sections, limit=5)
    goal_items = summary_points(goal_sections, limit=5)
    update_requirement_section(req_file, args.req_id, background_items, goal_items, doc_links, args.dry_run)

    req_row = find_requirement_row(req_file, args.req_id)
    if req_row is None:
        print("Error: requirement row not found after bundle creation")
        return 1

    current_title = get_cell(req_row.cells, req_row.header_map, ("标题", "需求标题")) or args.theme
    current_status = get_cell(req_row.cells, req_row.header_map, ("状态",)) or "planned"
    current_source = get_cell(req_row.cells, req_row.header_map, ("来源",)) or md_link(prd_file, project_paths.workspace_root)
    if "待补PRD文档.md" in current_source or "待补PRD文档" in current_source:
        current_source = md_link(prd_file, project_paths.workspace_root)
    current_task_board = get_cell(req_row.cells, req_row.header_map, ("任务拆解",)) or md_link(task_file, project_paths.workspace_root)

    sync_requirement_pool_entry(
        req_path=req_file,
        req_id=args.req_id,
        title=current_title,
        status=current_status,
        source=current_source,
        design_docs=design_doc_links,
        task_board=current_task_board,
        sync_date=args.date,
        dry_run=args.dry_run,
    )

    breakdown_link = md_link(breakdown, project_paths.workspace_root)
    task_pairs = generated_tasks(
        initial_task_id=args.initial_task_id,
        task_file=task_file,
        sync_date=args.date,
        breakdown_link=breakdown_link,
        function_items=function_items,
        top_modules=top_modules,
        child_map=child_map,
        use_cases=use_cases,
        blueprints=feature_blueprints,
    )

    for task_id, task in task_pairs:
        existing_title, existing_acceptance = current_task_content(task_file, task_id)
        title = choose_richer_text(existing_title, task.title)
        acceptance = choose_richer_text(existing_acceptance, task.acceptance)
        sync_task_board_entry(
            task_path=task_file,
            req_id=args.req_id,
            req_title=current_title,
            task_id=task_id,
            task_title=title,
            status="todo",
            acceptance=acceptance,
            doc_link=task.doc_link,
            sync_date=args.date,
            dry_run=args.dry_run,
        )

    legacy_req_id = derive_legacy_req_id(args.req_id, args.date)
    legacy_task_ids = derive_legacy_task_ids(args.initial_task_id, args.date, len(task_pairs))
    purge_legacy_task_board_rows(task_file, legacy_req_id, legacy_task_ids, args.dry_run)
    purge_legacy_task_index_rows(project_paths.tasks_index, legacy_task_ids, args.dry_run)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
