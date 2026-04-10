import React, { useState } from 'react';
import { projects as seedProjects, stageMeta, workspace } from './data';

const statusLabels = {
  all: '全部',
  todo: '待处理',
  doing: '进行中',
  review: '待审核',
  blocked: '阻塞中',
  done: '已完成',
};

const homePageSize = 8;

const demoProjectSpecs = [
  {
    id: 'oa',
    name: 'OA 协同平台',
    alias: 'OA',
    domain: '审批 / 通知 / 协作',
    owner: '陈默',
    lead: '产品 + 前端 + 后端',
    iteration: 'V1.2 审批流改造',
    release: '2026 Q2 - Iteration 1',
    stage: 'execution',
    gateStatus: '进行中',
    health: '良好',
    risk: '低',
    lastUpdated: '2026-04-10 09:38',
    sourcePath: '/Users/wucongpeng/Documents/jty-work/oa-platform',
    summary: '围绕审批链路、通知模板和协作记录做收口，当前聚焦审批流顺序与权限边界。',
    metrics: { totalTasks: 3, doing: 1, blocked: 0, review: 1, done: 1, evidenceCoverage: 72, releaseGate: '待验证' },
    currentTask: 'TASK-401',
    currentFocus: '审批链路顺序确认',
    blocker: '暂无阻塞',
    tasks: [
      ['TASK-401', 'REQ-301', '审批链路重排', 'doing', '王珊', 'P0', '字段回填口径待确认', '补齐回填规则后继续', '联调中'],
      ['TASK-402', 'REQ-301', '通知模板统一', 'review', '许衡', 'P2', '', '等待产品确认文案', '截图回归'],
      ['TASK-403', 'REQ-302', '权限边界校验', 'done', '李木', 'P1', '', '已完成归档', '验证记录 / 提交'],
    ],
    evidence: [
      { title: '联调记录', value: '3 条', detail: '审批链路与通知模板已联调' },
      { title: '证据完整度', value: '72%', detail: '还差最终确认材料' },
    ],
    risks: ['审批链路的字段回填需要再确认', '通知模板的文案边界还没冻结'],
    timeline: [
      { time: '09:38', label: '进入执行收口', desc: '审批链路重排开始进入联调。' },
      { time: '09:02', label: '更新任务看板', desc: 'TASK-402 进入 review。' },
    ],
    members: ['陈默', '王珊', '许衡', '李木'],
  },
  {
    id: 'crm',
    name: '客户关系平台',
    alias: 'CRM',
    domain: '客户 / 跟进 / 线索',
    owner: '周岚',
    lead: '产品 + 设计 + 后端',
    iteration: 'V2.1 线索重构',
    release: '2026 Q2 - Iteration 2',
    stage: 'requirement',
    gateStatus: '待人工审核',
    health: '观察中',
    risk: '中',
    lastUpdated: '2026-04-10 09:11',
    sourcePath: '/Users/wucongpeng/Documents/jty-work/crm',
    summary: '当前沉淀客户分层、跟进节奏和线索分发规则，处于需求治理收口阶段。',
    metrics: { totalTasks: 3, doing: 1, blocked: 1, review: 1, done: 0, evidenceCoverage: 56, releaseGate: '未进入' },
    currentTask: 'REQ-402',
    currentFocus: '客户分层规则治理',
    blocker: '客户分层规则尚未冻结，等待产品签字。',
    tasks: [
      ['TASK-421', 'REQ-402', '客户标签分层', 'doing', '方遥', 'P0', '分层规则待确认', '补齐分层口径后评审', '规则草案'],
      ['TASK-422', 'REQ-402', '线索跟进提醒', 'review', '唐婧', 'P2', '', '等待审核意见', '交接草稿'],
      ['TASK-423', 'REQ-403', '销售漏斗图表', 'blocked', '郑闻', 'P1', '图表字段尚未冻结', '补确认单后恢复', '阻塞说明已登记'],
    ],
    evidence: [
      { title: '需求交接', value: '待审核', detail: '不能直接进入 execution' },
      { title: '任务看板', value: '3 条', detail: '已同步到需求池' },
    ],
    risks: ['客户分层规则未冻结', '销售漏斗图表字段需要补确认'],
    timeline: [
      { time: '09:11', label: '更新需求池', desc: '客户分层和线索规则完成初稿。' },
      { time: '08:40', label: '停在审核门', desc: '等待确认后再开工。' },
    ],
    members: ['周岚', '方遥', '唐婧', '郑闻'],
  },
  {
    id: 'notify',
    name: '通知中心',
    alias: 'NFC',
    domain: '消息 / 推送 / 触达',
    owner: '林书',
    lead: '产品 + 前端 + 平台',
    iteration: 'V1.4 触达策略升级',
    release: '2026 Q2 - Iteration 1',
    stage: 'execution',
    gateStatus: '进行中',
    health: '良好',
    risk: '低',
    lastUpdated: '2026-04-10 08:52',
    sourcePath: '/Users/wucongpeng/Documents/jty-work/notification-center',
    summary: '围绕模板、触达策略和分组推送做收口，当前重点在渠道优先级和确认机制。',
    metrics: { totalTasks: 3, doing: 1, blocked: 0, review: 1, done: 1, evidenceCoverage: 81, releaseGate: '待验证' },
    currentTask: 'TASK-501',
    currentFocus: '触达策略优先级',
    blocker: '暂无阻塞',
    tasks: [
      ['TASK-501', 'REQ-501', '模板优先级排序', 'doing', '叶然', 'P1', '', '补齐优先级说明', '联调中'],
      ['TASK-502', 'REQ-501', '渠道确认弹窗', 'review', '沈宁', 'P2', '', '等待交互审核', '原型已更新'],
      ['TASK-503', 'REQ-502', '失败重试策略', 'done', '穆安', 'P1', '', '已完成归档', '验证通过'],
    ],
    evidence: [
      { title: '验证记录', value: '5 条', detail: '推送与重试均已覆盖' },
      { title: '证据完整度', value: '81%', detail: '只差最终发布说明' },
    ],
    risks: ['渠道优先级需要统一口径'],
    timeline: [
      { time: '08:52', label: '更新触达策略', desc: '模板优先级调整进入联调。' },
      { time: '07:40', label: '完成验证', desc: '失败重试策略验证通过。' },
    ],
    members: ['林书', '叶然', '沈宁', '穆安'],
  },
  {
    id: 'report',
    name: '报表中心',
    alias: 'RPT',
    domain: '指标 / 看板 / 导出',
    owner: '沈知',
    lead: '产品 + 数据 + 前端',
    iteration: 'V0.9 指标底座',
    release: '2026 Q2 - Iteration 1',
    stage: 'bootstrap',
    gateStatus: '待初始化',
    health: '需要关注',
    risk: '低',
    lastUpdated: '2026-04-10 08:30',
    sourcePath: '/Users/wucongpeng/Documents/jty-work/report-center',
    summary: '报表中心处于底座搭建阶段，重点是统一指标口径、图表规范和导出格式。',
    metrics: { totalTasks: 3, doing: 0, blocked: 0, review: 0, done: 1, evidenceCoverage: 34, releaseGate: '未进入' },
    currentTask: 'TASK-611',
    currentFocus: '指标口径统一',
    blocker: '底座连接方式待确认',
    tasks: [
      ['TASK-611', 'REQ-611', '指标口径对齐', 'todo', '待分配', 'P0', '', '先补齐数据字典', '待创建'],
      ['TASK-612', 'REQ-611', '图表规范统一', 'todo', '待分配', 'P1', '', '完成视觉口径后拆解', '待创建'],
      ['TASK-613', 'REQ-612', '导出格式设计', 'done', '贺然', 'P2', '', '已完成初版归档', '草案已冻结'],
    ],
    evidence: [
      { title: '底座状态', value: '待初始化', detail: '先补齐仓库底座和项目事实' },
      { title: '证据完整度', value: '34%', detail: '主要是需求草稿' },
    ],
    risks: ['指标口径不统一会影响后续所有图表'],
    timeline: [
      { time: '08:30', label: '建立需求线索', desc: '指标口径与导出格式进入初稿。' },
      { time: '07:55', label: '底座准备中', desc: '还没进入 execution。' },
    ],
    members: ['沈知', '贺然', '待分配'],
  },
  {
    id: 'docs',
    name: '文档知识库',
    alias: 'DOC',
    domain: '知识 / 搜索 / 标签',
    owner: '宋雅',
    lead: '产品 + 内容 + 工程',
    iteration: 'V1.6 知识检索优化',
    release: '2026 Q2 - Iteration 2',
    stage: 'execution',
    gateStatus: '进行中',
    health: '良好',
    risk: '低',
    lastUpdated: '2026-04-10 08:12',
    sourcePath: '/Users/wucongpeng/Documents/jty-work/docs-hub',
    summary: '围绕文档检索、标签体系和知识归档继续推进，重点在内容分类和检索体验。',
    metrics: { totalTasks: 3, doing: 1, blocked: 0, review: 1, done: 1, evidenceCoverage: 76, releaseGate: '待验证' },
    currentTask: 'TASK-701',
    currentFocus: '标签体系整理',
    blocker: '暂无阻塞',
    tasks: [
      ['TASK-701', 'REQ-701', '文档标签体系', 'doing', '陶青', 'P1', '', '补齐标签映射表', '整理中'],
      ['TASK-702', 'REQ-701', '检索结果排序', 'review', '罗晴', 'P2', '', '等待体验确认', '回归截图'],
      ['TASK-703', 'REQ-702', '归档权限控制', 'done', '江屿', 'P1', '', '归档完成', '验证通过'],
    ],
    evidence: [
      { title: '归档记录', value: '4 条', detail: '检索与标签整理已覆盖' },
      { title: '证据完整度', value: '76%', detail: '只差归档说明' },
    ],
    risks: ['标签体系需要与搜索排序同步'],
    timeline: [
      { time: '08:12', label: '更新标签体系', desc: '文档分类规则继续细化。' },
      { time: '07:34', label: '进入执行收口', desc: '检索排序进入 review。' },
    ],
    members: ['宋雅', '陶青', '罗晴', '江屿'],
  },
  {
    id: 'ops',
    name: '运维告警台',
    alias: 'OPS',
    domain: '告警 / 升级 / 值班',
    owner: '杜衡',
    lead: '产品 + 运维 + 后端',
    iteration: 'V1.3 告警路由重构',
    release: '2026 Q2 - Iteration 2',
    stage: 'requirement',
    gateStatus: '待人工审核',
    health: '观察中',
    risk: '中',
    lastUpdated: '2026-04-10 07:58',
    sourcePath: '/Users/wucongpeng/Documents/jty-work/ops-alerts',
    summary: '围绕告警路由、升级策略和值班分派收敛需求，目前停在人工审核门。',
    metrics: { totalTasks: 3, doing: 1, blocked: 1, review: 1, done: 0, evidenceCoverage: 49, releaseGate: '未进入' },
    currentTask: 'REQ-801',
    currentFocus: '告警升级策略确认',
    blocker: '告警升级策略待确认',
    tasks: [
      ['TASK-801', 'REQ-801', '告警等级映射', 'doing', '杜衡', 'P0', '升级策略待确认', '补齐映射表后审核', '规则草案'],
      ['TASK-802', 'REQ-801', '值班推送节奏', 'blocked', '贺然', 'P1', '推送节奏未冻结', '补交接说明后恢复', '阻塞说明已登记'],
      ['TASK-803', 'REQ-802', '事件归档页', 'review', '秦朔', 'P2', '', '等待审核反馈', '草稿已提交'],
    ],
    evidence: [
      { title: '需求交接', value: '待审核', detail: '暂不进入 execution' },
      { title: '证据完整度', value: '49%', detail: '告警策略材料还没收齐' },
    ],
    risks: ['告警升级策略未冻结', '值班推送节奏可能影响交接'],
    timeline: [
      { time: '07:58', label: '更新需求草案', desc: '告警等级映射进入 review。' },
      { time: '06:50', label: '停在审核门', desc: '等待确认后再开工。' },
    ],
    members: ['杜衡', '贺然', '秦朔'],
  },
];

function App() {
  const [projects, setProjects] = useState(() => buildInitialProjects(seedProjects));
  const [view, setView] = useState('home');
  const [activeProjectId, setActiveProjectId] = useState(seedProjects[0]?.id ?? '');
  const [homePage, setHomePage] = useState(0);
  const [taskFilter, setTaskFilter] = useState('all');
  const [composerOpen, setComposerOpen] = useState(false);
  const [draft, setDraft] = useState(createProjectDraft());
  const [inspectorTab, setInspectorTab] = useState('summary');

  const activeProject =
    projects.find((project) => project.id === activeProjectId) ?? projects[0] ?? null;

  const workspaceStats = summarizeWorkspace(projects);
  const pageCount = Math.max(1, Math.ceil(projects.length / homePageSize));
  const currentPage = Math.min(homePage, pageCount - 1);
  const visibleProjects = projects.slice(currentPage * homePageSize, currentPage * homePageSize + homePageSize);
  const isLastPage = currentPage === pageCount - 1;
  const taskCounts = countTasks(activeProject?.tasks ?? []);
  const visibleTasks =
    activeProject && taskFilter === 'all'
      ? activeProject.tasks
      : activeProject?.tasks.filter((task) => task.status === taskFilter) ?? [];

  function openProject(projectId) {
    setActiveProjectId(projectId);
    setView('project');
    setTaskFilter('all');
    setInspectorTab('summary');
  }

  function backToHome() {
    setView('home');
    setComposerOpen(false);
  }

  function handleRefresh() {
    const now = formatTimestamp(new Date());
    setProjects((current) =>
      current.map((project) =>
        project.id === activeProject?.id ? { ...project, lastUpdated: now } : project,
      ),
    );
  }

  function handleCreateProject(event) {
    event.preventDefault();
    const sourcePath = draft.sourcePath.trim();
    if (!sourcePath) return;

    const createdProject = buildProjectFromDraft(draft, projects.length + 1);
    setProjects((current) => [createdProject, ...current]);
    setActiveProjectId(createdProject.id);
    setHomePage(0);
    setTaskFilter('all');
    setInspectorTab('summary');
    setDraft(createProjectDraft());
    setComposerOpen(false);
    setView('project');
  }

  return (
    <div className={`app-shell app-shell--${view}`}>
      <div className="ambient ambient-a" />
      <div className="ambient ambient-b" />

      <header className="topbar">
        <div>
          <h1>{view === 'home' ? '项目驾驶舱' : activeProject?.name ?? '项目页'}</h1>
        </div>

        <div className="topbar-actions">
          <MetaChip label="项目数" value={`${workspaceStats.total} 个`} />
          <MetaChip label="阻塞中" value={`${workspaceStats.blocked} 个`} tone="danger" />
          <MetaChip label="最近更新" value={workspaceStats.latestUpdated} tone="muted" />
          {view === 'project' ? (
            <button className="ghost-button" type="button" onClick={backToHome}>
              ← 返回首页
            </button>
          ) : null}
          <button className="primary-button" type="button" onClick={() => setComposerOpen(true)}>
            + 新建项目
          </button>
        </div>
      </header>

      {view === 'home' ? (
        <main className="dashboard dashboard--home">
          <section className="panel home-board">
            <div className="home-board__head">
              <div className="home-board__spacer" />

              {pageCount > 1 ? (
                <div className="home-board__pager">
                  <button className="ghost-button" type="button" onClick={() => setHomePage((page) => Math.max(0, page - 1))} disabled={currentPage === 0}>
                    上一页
                  </button>
                  <span>
                    第 {currentPage + 1} 页 / 共 {pageCount} 页
                  </span>
                  <button
                    className="ghost-button"
                    type="button"
                    onClick={() => setHomePage((page) => Math.min(pageCount - 1, page + 1))}
                    disabled={currentPage >= pageCount - 1}
                  >
                    下一页
                  </button>
                </div>
              ) : null}
            </div>

            <div className="home-grid">
              {visibleProjects.map((project, index) => (
                <ProjectTile
                  key={project.id}
                  project={project}
                  delay={index * 60}
                  onClick={() => openProject(project.id)}
                />
              ))}

              {isLastPage ? (
                <button className="project-tile project-tile--new" type="button" onClick={() => setComposerOpen(true)}>
                  <span>+</span>
                  <strong>新建项目</strong>
                </button>
              ) : null}
            </div>
          </section>
        </main>
      ) : (
        <main className="dashboard dashboard--project">
          <aside className="panel project-rail">
            <SectionTitle
              title="项目概览"
              action={
                <button className="ghost-button" type="button" onClick={handleRefresh}>
                  同步状态
                </button>
              }
            />

            <div className="project-rail__hero">
              <span className="project-alias">{activeProject?.alias ?? 'NEW'}</span>
              <h2>{activeProject?.name ?? '未选择项目'}</h2>
            </div>

            <div className="project-rail__chips">
              <MetaChip label="阶段" value={activeProject ? stageTitle(activeProject.stage) : '—'} />
              <MetaChip label="门禁" value={activeProject?.gateStatus ?? '—'} />
              <MetaChip label="任务" value={String(activeProject?.tasks?.length ?? 0)} tone="muted" />
            </div>

            <div className="stage-strip stage-strip--stacked">
              {stageMeta.map((stage, index) => {
                const state = activeProject?.stageStates?.[stage.key] ?? 'next';
                return (
                  <article key={stage.key} className={`stage-rail-card stage-${stage.color} state-${state}`}>
                    <div className="stage-rail-card__head">
                      <span>{String(index + 1).padStart(2, '0')}</span>
                      <em>{stageStateLabel(state)}</em>
                    </div>
                    <strong>{stage.title}</strong>
                  </article>
                );
              })}
            </div>
          </aside>

          <section className="panel panel--tasks">
            <div className="panel-head">
              <div className="panel-head__spacer" />

              <div className="filter-row">
                {(['all', 'todo', 'doing', 'review', 'blocked', 'done']).map((status) => (
                  <button
                    key={status}
                    className={`filter-chip ${taskFilter === status ? 'is-active' : ''}`}
                    type="button"
                    onClick={() => setTaskFilter(status)}
                  >
                    {statusLabels[status]}
                    <span>{status === 'all' ? activeProject?.tasks?.length ?? 0 : taskCounts[status]}</span>
                  </button>
                ))}
              </div>
            </div>

            <div className="task-list">
              {visibleTasks.length ? (
                visibleTasks.map((task, index) => <TaskCard key={task.id} task={task} delay={index * 50} />)
              ) : (
                <div className="empty-state">
                  当前筛选下没有任务。你可以切换筛选项，或者先新建一个项目补充任务。
                </div>
              )}
            </div>
          </section>

          <aside className="panel inspector">
            <SectionTitle title="项目侧栏" />

            <div className="inspector-tabs">
              {[
                ['summary', '概览'],
                ['evidence', '证据'],
                ['risk', '风险'],
                ['timeline', '时间线'],
                ['team', '团队'],
              ].map(([key, label]) => (
                <button
                  key={key}
                  type="button"
                  className={`inspector-tab ${inspectorTab === key ? 'is-active' : ''}`}
                  onClick={() => setInspectorTab(key)}
                >
                  {label}
                </button>
              ))}
            </div>

            <div className="inspector-content">
              {inspectorTab === 'summary' ? (
                <section className="inspector-block">
                  <div className="summary-grid">
                    <MetricCard label="总任务" value={activeProject?.metrics.totalTasks ?? 0} />
                    <MetricCard label="进行中" value={activeProject?.metrics.doing ?? 0} />
                    <MetricCard label="待审核" value={activeProject?.metrics.review ?? 0} />
                    <MetricCard label="阻塞" value={activeProject?.metrics.blocked ?? 0} danger />
                  </div>

                  <div className="gate-box">
                    <span>当前门禁</span>
                    <strong>{activeProject?.gateStatus ?? '未选择项目'}</strong>
                    <p>{activeProject?.blocker ?? '先选择一个项目查看当前门禁。'}</p>
                  </div>
                </section>
              ) : null}

              {inspectorTab === 'evidence' ? (
                <section className="inspector-block">
                  <div className="evidence-list">
                    {(activeProject?.evidence ?? []).length ? (
                      activeProject.evidence.map((item) => (
                        <article key={item.title} className="mini-card">
                          <span>{item.title}</span>
                          <strong>{item.value}</strong>
                          <p>{item.detail}</p>
                        </article>
                      ))
                    ) : (
                      <div className="empty-state compact">暂无证据记录</div>
                    )}
                  </div>
                </section>
              ) : null}

              {inspectorTab === 'risk' ? (
                <section className="inspector-block">
                  <ul className="risk-list">
                    {(activeProject?.risks ?? []).length ? (
                      activeProject.risks.map((risk) => <li key={risk}>{risk}</li>)
                    ) : (
                      <li>暂无风险</li>
                    )}
                  </ul>
                </section>
              ) : null}

              {inspectorTab === 'timeline' ? (
                <section className="inspector-block">
                  <div className="timeline">
                    {(activeProject?.timeline ?? []).length ? (
                      activeProject.timeline.map((item, index) => (
                        <article key={`${item.time}-${index}`} className="timeline-item">
                          <span className="timeline-time">{item.time}</span>
                          <div>
                            <h4>{item.label}</h4>
                            <p>{item.desc}</p>
                          </div>
                        </article>
                      ))
                    ) : (
                      <div className="empty-state compact">暂无时间线</div>
                    )}
                  </div>
                </section>
              ) : null}

              {inspectorTab === 'team' ? (
                <section className="inspector-block">
                  <div className="member-list">
                    {(activeProject?.members ?? []).length ? (
                      activeProject.members.map((member) => (
                        <span key={member} className="member-pill">
                          {member}
                        </span>
                      ))
                    ) : (
                      <span className="member-pill">暂无成员</span>
                    )}
                  </div>
                </section>
              ) : null}
            </div>
          </aside>
        </main>
      )}

      {composerOpen ? (
        <div className="composer-overlay" onMouseDown={() => setComposerOpen(false)}>
          <form className="composer-modal panel" onMouseDown={(event) => event.stopPropagation()} onSubmit={handleCreateProject}>
            <div className="composer-modal__head">
              <div>
                <p className="eyebrow">新建项目</p>
                <h2>只填项目地址</h2>
                <p>系统会根据路径自动生成项目名、简称和基础卡片。</p>
              </div>
              <button className="ghost-button" type="button" onClick={() => setComposerOpen(false)}>
                关闭
              </button>
            </div>

            <div className="field">
              <label>项目地址</label>
              <input
                value={draft.sourcePath}
                onChange={(event) => setDraft({ ...draft, sourcePath: event.target.value })}
                placeholder="例如 /Users/wucongpeng/Documents/jty-work/erp-finance"
              />
            </div>

            <button className="submit-button" type="submit">
              创建项目
            </button>
          </form>
        </div>
      ) : null}
    </div>
  );
}

function createProjectDraft() {
  return {
    sourcePath: '',
  };
}

function buildInitialProjects(baseProjects) {
  return [...baseProjects.map(cloneProject), ...demoProjectSpecs.map(createProjectFromSpec)];
}

function cloneProject(project) {
  return {
    ...project,
    metrics: { ...project.metrics },
    stageStates: { ...project.stageStates },
    tasks: project.tasks.map((task) => ({ ...task })),
    evidence: project.evidence.map((item) => ({ ...item })),
    risks: [...project.risks],
    timeline: project.timeline.map((item) => ({ ...item })),
    members: [...project.members],
  };
}

function createProjectFromSpec(spec) {
  return {
    ...spec,
    metrics: { ...spec.metrics },
    stageStates: buildStageStates(spec.stage),
    tasks: spec.tasks.map(([id, reqId, title, status, owner, priority, blocker, next, evidence]) => ({
      id,
      reqId,
      title,
      status,
      owner,
      priority,
      blocker,
      next,
      evidence,
    })),
    evidence: spec.evidence.map((item) => ({ ...item })),
    risks: [...spec.risks],
    timeline: spec.timeline.map((item) => ({ ...item })),
    members: [...spec.members],
  };
}

function buildProjectFromDraft(draft, index) {
  const sourcePath = draft.sourcePath.trim();
  const localProfile = resolveLocalProjectProfile({ sourcePath });

  if (localProfile) {
    return localProfile;
  }

  const resolvedName = shortLabel(sourcePath) || `未命名项目 ${index}`;
  const aliasSource = resolvedName || shortLabel(sourcePath) || `P${index}`;
  const alias = aliasSource.slice(0, 4).toUpperCase() || `P${index}`;
  const owner = '待分配';
  const domain = '待补充';
  const iteration = 'V1.0 初始迭代';
  const stage = 'requirement';
  const stageStates = buildStageStates(stage);
  const summary = buildProjectSummary(resolvedName, domain, owner, stage, sourcePath);

  return {
    id: `project-${Date.now()}-${index}`,
    name: resolvedName,
    alias,
    sourcePath,
    domain,
    owner,
    lead: '产品 + 设计 + 工程',
    iteration,
    release: '未设置',
    stage,
    gateStatus: stage === 'bootstrap' ? '待初始化' : stage === 'requirement' ? '待人工审核' : '待验证',
    health: '新建',
    risk: stage === 'execution' ? '中' : '低',
    lastUpdated: formatTimestamp(new Date()),
    summary,
    metrics: {
      totalTasks: 0,
      doing: 0,
      blocked: 0,
      review: 0,
      done: 0,
      evidenceCoverage: 0,
      releaseGate: stage === 'execution' ? '待验证' : '未进入',
    },
    stageStates,
    currentTask: '待创建',
    currentFocus: '待补充',
    blocker: '暂无阻塞',
    tasks: [],
    evidence: [
      {
        title: '创建状态',
        value: '已创建',
        detail: '请继续补充需求、任务和证据。',
      },
    ],
    risks: ['新建项目尚未拆任务，后续需要补充需求池和任务看板。'],
    timeline: [
      {
        time: formatClock(new Date()),
        label: '创建项目',
        desc: '通过驾驶舱新增入口创建。',
      },
    ],
    members: [owner],
  };
}

function resolveLocalProjectProfile({ sourcePath }) {
  const normalizedSource = shortLabel(sourcePath).toLowerCase();
  const normalizedPath = sourcePath.toLowerCase();
  const projectTemplate = localProjectTemplates.find((item) => {
    return item.match(normalizedSource, normalizedPath);
  });

  if (!projectTemplate) return null;

  const now = new Date();
  return projectTemplate.build({ now, sourcePath });
}

const localProjectTemplates = [
  {
    match(normalizedSource, normalizedPath) {
      return (
        normalizedSource === 'erp-finance' ||
        normalizedSource === 'erp-finance-web' ||
        normalizedPath.includes('/erp-finance')
      );
    },
    build({ now, sourcePath }) {
      return createLocalSnapshot({
        id: `project-erp-finance-${now.getTime()}`,
        name: 'ERP 财务管理系统',
        alias: 'ERP',
        sourcePath: sourcePath.trim() || '/Users/wucongpeng/Documents/jty-work/erp-finance',
        domain: '财务 / 凭证 / 报销',
        owner: '财务协作组',
        lead: '产品 + 前端 + 后端 + 数据',
        iteration: 'V3.0 workflow 接入',
        release: '本地项目',
        stage: 'requirement',
        gateStatus: '进行中',
        health: '观察中',
        risk: '中',
        lastUpdated: formatTimestamp(now),
        summary:
          '本地财务系统已接入 workflow 底座与运行态记忆，当前围绕需求治理、任务回写和验证收口推进。',
        metrics: {
          totalTasks: 5,
          doing: 2,
          blocked: 1,
          review: 1,
          done: 1,
          evidenceCoverage: 64,
          releaseGate: '待验证',
        },
        currentTask: 'TASK-901',
        currentFocus: 'workflow 状态快照接入',
        blocker: '本地项目快照已创建，待继续补齐任务记忆与证据回写。',
        tasks: [
          ['TASK-901', 'REQ-901', 'workflow 底座接入', 'done', '平台', 'P1', '', '已完成并归档', 'PROJECT_CONTEXT / AGENTS 已接入'],
          ['TASK-902', 'REQ-902', '需求池与任务看板同步', 'doing', '治理', 'P0', '需要同步本地需求目录', '补齐 doc/requirements/', '需求治理目录已定位'],
          ['TASK-903', 'REQ-903', '任务记忆落点确认', 'review', '记忆', 'P1', '', '确认 .ai/memory/tasks/ 归档策略', '.ai/memory 目录已识别'],
          ['TASK-904', 'REQ-904', '项目路径自动扫描', 'doing', '驾驶舱', 'P0', '需要补充更多目录映射规则', '扩展目录扫描规则', 'sourcePath 已接入'],
          ['TASK-905', 'REQ-905', '发布闸门与验证记录', 'blocked', '验证', 'P1', '最终验证记录待补齐', '补齐证据后恢复', '待生成'],
        ],
        evidence: [
          { title: '底座状态', value: '已接入', detail: 'PROJECT_CONTEXT / AGENTS 已识别' },
          { title: '证据完整度', value: '64%', detail: '缺少最终验证记录与发布说明' },
          { title: '目录落点', value: '已定位', detail: 'doc/requirements 与 .ai/memory 可继续接入' },
        ],
        risks: [
          '需求池与任务看板仍需要进一步结构化',
          '任务记忆与知识落点需要和本地规范对齐',
          '验证记录与发布闸门还未完全闭环',
        ],
        timeline: [
          { time: formatClock(now), label: '创建项目快照', desc: '根据本地路径自动挂载 ERP 财务管理系统。' },
          { time: '09:41', label: '接入工作流事实', desc: '识别 PROJECT_CONTEXT 与 AGENTS。' },
          { time: '09:12', label: '扫描项目目录', desc: '准备对 .ai 与 requirements 目录继续接入。' },
        ],
        members: ['财务协作组', '治理', '记忆', '验证'],
      });
    },
  },
];

function createLocalSnapshot(spec) {
  return {
    ...spec,
    metrics: { ...spec.metrics },
    stageStates: buildStageStates(spec.stage),
    tasks: spec.tasks.map(([id, reqId, title, status, owner, priority, blocker, next, evidence]) => ({
      id,
      reqId,
      title,
      status,
      owner,
      priority,
      blocker,
      next,
      evidence,
    })),
    evidence: spec.evidence.map((item) => ({ ...item })),
    risks: [...spec.risks],
    timeline: spec.timeline.map((item) => ({ ...item })),
    members: [...spec.members],
  };
}

function buildProjectSummary(name, domain, owner, stage, sourcePath) {
  const stageText =
    stage === 'bootstrap'
      ? '先补齐仓库底座'
      : stage === 'requirement'
        ? '先治理需求和任务'
        : '进入执行收口';
  const pathText = sourcePath ? `，路径为 ${sourcePath}` : '';

  return `${name} 主要覆盖 ${domain}，当前由 ${owner} 负责${pathText}，阶段目标是${stageText}。`;
}

function formatTimestamp(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  const hour = String(date.getHours()).padStart(2, '0');
  const minute = String(date.getMinutes()).padStart(2, '0');
  return `${year}-${month}-${day} ${hour}:${minute}`;
}

function formatClock(date) {
  const hour = String(date.getHours()).padStart(2, '0');
  const minute = String(date.getMinutes()).padStart(2, '0');
  return `${hour}:${minute}`;
}

function buildStageStates(stage) {
  if (stage === 'bootstrap') {
    return { bootstrap: 'current', requirement: 'next', execution: 'next' };
  }

  if (stage === 'execution') {
    return { bootstrap: 'done', requirement: 'done', execution: 'current' };
  }

  return { bootstrap: 'done', requirement: 'current', execution: 'next' };
}

function summarizeWorkspace(projectList) {
  const total = projectList.length;
  const blocked = projectList.filter((project) => project.risk === '高' || project.metrics.blocked > 0).length;
  const latestUpdated = projectList.reduce((latest, project) => {
    if (!latest) return project.lastUpdated;
    return project.lastUpdated > latest ? project.lastUpdated : latest;
  }, '');

  return {
    total,
    blocked,
    latestUpdated: latestUpdated || workspace.updatedAt,
  };
}

function countTasks(tasks) {
  return tasks.reduce(
    (acc, task) => {
      acc[task.status] += 1;
      return acc;
    },
    {
      todo: 0,
      doing: 0,
      review: 0,
      blocked: 0,
      done: 0,
    },
  );
}

function stageTitle(stage) {
  return stageMeta.find((item) => item.key === stage)?.title ?? stage;
}

function stageStateLabel(state) {
  if (state === 'done') return '已完成';
  if (state === 'current') return '当前阶段';
  return '下一阶段';
}

function shortLabel(value) {
  if (!value) return '';
  const parts = String(value).replace(/\\/g, '/').split('/').filter(Boolean);
  return parts[parts.length - 1] ?? String(value);
}

function riskClass(risk) {
  if (risk === '高') return 'high';
  if (risk === '中') return 'mid';
  return 'low';
}

function SectionTitle({ title, caption, action }) {
  return (
    <div className="section-title">
      <div>
        <h3>{title}</h3>
        {caption ? <p>{caption}</p> : null}
      </div>
      {action ? <div className="section-action">{action}</div> : null}
    </div>
  );
}

function MetaChip({ label, value, tone }) {
  return (
    <div className={`meta-chip ${tone ? `tone-${tone}` : ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function MetricCard({ label, value, accent = false, danger = false }) {
  return (
    <article className={`metric-card ${accent ? 'is-accent' : ''} ${danger ? 'is-danger' : ''}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </article>
  );
}

function ProjectTile({ project, onClick, delay }) {
  return (
    <button className={`project-tile stage-${project.stage} project-risk-${riskClass(project.risk)}`} type="button" onClick={onClick} style={{ animationDelay: `${delay}ms` }}>
      <div className="project-tile__top">
        <span className="project-tile__alias">{project.alias}</span>
        <span className={`risk-pill risk-${riskClass(project.risk)}`}>{project.risk}</span>
      </div>

      <h3>{project.name}</h3>
      <p>{project.summary}</p>

      <div className="project-tile__meta">
        <span>{stageTitle(project.stage)}</span>
        <span>{project.tasks.length} 任务</span>
      </div>

      <div className="project-tile__foot">
        <strong>{project.lastUpdated}</strong>
        <small>{shortLabel(project.sourcePath) || project.owner}</small>
      </div>
    </button>
  );
}

function TaskCard({ task, delay }) {
  return (
    <article className="task-card" style={{ animationDelay: `${delay}ms` }}>
      <div className="task-card__top">
        <div>
          <span className="task-id">{task.id}</span>
          <h4>{task.title}</h4>
        </div>
        <span className={`status-pill status-${task.status}`}>{statusLabels[task.status]}</span>
      </div>

      <div className="task-meta">
        <span>{task.reqId}</span>
        <span>{task.owner}</span>
        <span>{task.priority}</span>
      </div>

      <div className="task-flow">
        <div>
          <span>下一步</span>
          <strong>{task.next}</strong>
        </div>
        <div>
          <span>证据</span>
          <strong>{task.evidence}</strong>
        </div>
      </div>

      {task.blocker ? <p className="task-blocker">{task.blocker}</p> : null}
    </article>
  );
}

export default App;
