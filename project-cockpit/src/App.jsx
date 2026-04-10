import React, { startTransition, useEffect, useState } from 'react';
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
  const [syncing, setSyncing] = useState(false);
  const [composerError, setComposerError] = useState('');

  const activeProject =
    projects.find((project) => project.id === activeProjectId) ?? projects[0] ?? null;

  const workspaceStats = summarizeWorkspace(projects);
  const pageCount = Math.max(1, Math.ceil(projects.length / homePageSize));
  const currentPage = Math.min(homePage, pageCount - 1);
  const visibleProjects = projects.slice(currentPage * homePageSize, currentPage * homePageSize + homePageSize);
  const isLastPage = currentPage === pageCount - 1;
  const taskCounts = countTasks(activeProject?.tasks ?? []);
  const orderedTasks = activeProject ? rankTasks(activeProject.tasks ?? [], activeProject.currentTask) : [];
  const visibleTasks =
    activeProject && taskFilter === 'all'
      ? getTaskWindow(orderedTasks, activeProject)
      : getTaskWindow((activeProject?.tasks ?? []).filter((task) => task.status === taskFilter), activeProject);

  useEffect(() => {
    const localProjects = projects.filter((project) => project.sourceType === 'local');
    if (!localProjects.length) return undefined;

    const intervalId = window.setInterval(() => {
      syncProjects(localProjects);
    }, 15000);

    return () => window.clearInterval(intervalId);
  }, [projects]);

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

  async function syncProjects(targetProjects) {
    const syncTargets = targetProjects.filter((project) => project.sourceType === 'local' && project.sourcePath);
    if (!syncTargets.length) return;

    setSyncing(true);
    const syncedProjects = await Promise.all(
      syncTargets.map(async (project) => {
        const snapshot = await fetchProjectSnapshot(project.sourcePath);
        if (!snapshot) {
          return {
            ...project,
            syncStatus: 'stale',
            syncLabel: '同步失败',
          };
        }
        return mergeProjectSnapshot(project, snapshot);
      }),
    );

    startTransition(() => {
      setProjects((current) =>
        current.map((project) => syncedProjects.find((item) => item.id === project.id) ?? project),
      );
    });
    setSyncing(false);
  }

  async function handleRefresh() {
    if (activeProject?.sourceType === 'local') {
      await syncProjects([activeProject]);
      return;
    }

    const now = formatTimestamp(new Date());
    setProjects((current) =>
      current.map((project) =>
        project.id === activeProject?.id
          ? { ...project, lastUpdated: now, syncLabel: '手动刷新', syncStatus: 'fresh' }
          : project,
      ),
    );
  }

  async function handleCreateProject(event) {
    event.preventDefault();
    const sourcePath = draft.sourcePath.trim();
    if (!sourcePath) return;

    setComposerError('');
    const createdProject = await buildProjectFromDraft(draft, projects.length + 1);
    if (!createdProject) {
      setComposerError('项目路径不可用，或者本地状态文件暂时无法读取。');
      return;
    }

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
          <h1>{view === 'home' ? '项目驾驶舱' : getProjectDisplayName(activeProject)}</h1>
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
                  {syncing ? '同步中' : '同步状态'}
                </button>
              }
            />

            <div className="project-rail__hero">
              <span className="project-alias">{activeProject?.alias ?? 'NEW'}</span>
              <h2>{getProjectDisplayName(activeProject)}</h2>
            </div>

            <div className="project-rail__chips">
              <MetaChip label="阶段" value={activeProject ? stageTitle(activeProject.stage) : '—'} />
              <MetaChip label="门禁" value={activeProject?.gateStatus ?? '—'} />
              <MetaChip label="任务" value={String(activeProject?.tasks?.length ?? 0)} tone="muted" />
            </div>

            <div className="project-rail__chips project-rail__chips--single">
              <MetaChip label="同步" value={activeProject?.syncLabel ?? '未同步'} tone={activeProject?.syncStatus === 'stale' ? 'danger' : 'muted'} />
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
            <div className="focus-banner">
              <div className="focus-banner__main">
                <div className="focus-banner__headline">
                  <span className="focus-banner__label">当前焦点</span>
                  <span className={`status-pill status-${activeProject?.currentTaskStatus || inferProjectStatus(activeProject)}`}>
                    {statusLabels[activeProject?.currentTaskStatus || inferProjectStatus(activeProject)] || activeProject?.gateStatus || '未同步'}
                  </span>
                </div>
                <strong>{activeProject?.currentFocus ?? '等待下一步'}</strong>
                <p>{activeProject?.blocker ?? '暂无阻塞'}</p>
              </div>

              <div className="focus-banner__meta">
                <div>
                  <span>当前任务</span>
                  <strong>{activeProject?.currentTask ?? '未设置'}</strong>
                </div>
                <div>
                  <span>证据</span>
                  <strong>{activeProject?.evidenceTotal ?? activeProject?.evidence?.length ?? 0} 条</strong>
                </div>
              </div>
            </div>

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

            {composerError ? <p className="composer-error">{composerError}</p> : null}

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
    sourceType: project.sourceType || 'demo',
    syncStatus: project.syncStatus || 'fresh',
    syncLabel: project.syncLabel || '演示数据',
    evidenceTotal: project.evidenceTotal ?? project.evidence.length,
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
    sourceType: spec.sourceType || 'demo',
    syncStatus: spec.syncStatus || 'fresh',
    syncLabel: spec.syncLabel || '演示数据',
    evidenceTotal: spec.evidenceTotal ?? spec.evidence.length,
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

async function buildProjectFromDraft(draft, index) {
  const sourcePath = draft.sourcePath.trim();
  const snapshot = await fetchProjectSnapshot(sourcePath);

  if (snapshot) {
    return mapProjectSnapshotToProject(snapshot, index);
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
    sourceType: 'local',
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
    syncStatus: 'stale',
    syncLabel: '未检测到状态文件',
    evidenceTotal: 0,
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
        value: '未初始化',
        detail: '当前路径下还没有 project-state.json，先执行 workflow-bootstrap。',
      },
    ],
    risks: ['当前项目还没有状态骨架，驾驶舱无法读取动态任务数据。'],
    timeline: [
      {
        time: formatClock(new Date()),
        label: '创建项目',
        desc: '路径已接入，但还未检测到 project-state.json。',
      },
    ],
    members: [owner],
  };
}

async function fetchProjectSnapshot(sourcePath) {
  const normalizedPath = sourcePath.trim();
  if (!normalizedPath) return null;

  try {
    const response = await fetch(`/api/project-state?path=${encodeURIComponent(normalizedPath)}`);
    const result = await response.json();
    if (!response.ok || result.error) {
      return {
        found: false,
        projectPath: normalizedPath,
        error: result.error || '读取失败',
      };
    }
    return result;
  } catch (error) {
    return {
      found: false,
      projectPath: normalizedPath,
      error: error instanceof Error ? error.message : '读取失败',
    };
  }
}

function mapProjectSnapshotToProject(snapshot, index, existingProject) {
  const sourcePath = snapshot.projectPath;
  const state = snapshot.state;

  if (!snapshot.found || !state) {
    return existingProject
      ? {
          ...existingProject,
          syncStatus: 'stale',
          syncLabel: snapshot.error ? '读取失败' : '未检测到状态文件',
          lastUpdated: existingProject.lastUpdated,
          blocker: snapshot.error || '当前路径下还没有状态骨架',
        }
      : buildProjectFromMissingState(sourcePath, index, snapshot.error);
  }

  const project = state.project ?? {};
  const workflow = state.workflow ?? {};
  const metrics = state.metrics ?? {};
  const evidence = Array.isArray(state.evidence) ? state.evidence : [];
  const risks = Array.isArray(state.risks) ? state.risks : [];
  const timeline = Array.isArray(state.timeline) ? state.timeline : [];
  const tasks = Array.isArray(state.tasks) ? state.tasks : [];
  const name = project.name || shortLabel(sourcePath) || `本地项目 ${index}`;
  const alias = buildAlias(name, sourcePath, index);

  return {
    id: existingProject?.id || `project-${Date.now()}-${index}`,
    name,
    alias,
    sourcePath,
    sourceType: 'local',
    domain: project.docsRoot || '本地项目',
    owner: existingProject?.owner || '本地项目',
    lead: `${project.language || '未知语言'} / ${project.buildTool || '未知构建'}`,
    iteration: workflow.currentMode || '运行态同步',
    release: project.prdDirectory || '未设置',
    stage: normalizeStage(workflow.stage),
    gateStatus: workflow.gateStatus || '未同步',
    health: workflow.health || '待扫描',
    risk: normalizeRisk(workflow.risk),
    lastUpdated: formatSyncTime(state.sync?.lastSyncAt || snapshot.updatedAt),
    summary: buildSnapshotSummary(name, project, workflow),
    syncStatus: normalizeSyncStatus(state.sync?.status),
    syncLabel: buildSyncLabel(state.sync),
    evidenceTotal: evidence.length,
    metrics: {
      totalTasks: metrics.totalTasks || tasks.length,
      doing: metrics.doing || 0,
      blocked: metrics.blocked || 0,
      review: metrics.review || 0,
      done: metrics.done || 0,
      evidenceCoverage: metrics.evidenceCoverage || 0,
      releaseGate: workflow.stage === 'execution' ? workflow.gateStatus || '进行中' : '未进入',
    },
    stageStates: buildStageStates(normalizeStage(workflow.stage)),
    currentTask: workflow.currentTaskId || '未设置',
    currentFocus: workflow.currentTaskTitle || workflow.currentReqTitle || '等待下一步',
    blocker: risks[0]?.text || snapshot.error || '暂无阻塞',
    tasks: tasks.map((task) => ({
      id: task.taskId || '未命名任务',
      reqId: task.reqId || 'REQ-待补充',
      title: task.title || '未命名任务',
      status: normalizeTaskStatus(task.status),
      owner: task.owner || '',
      priority: task.priority || '',
      blocker: '',
      next: task.acceptance || '继续推进',
      evidence: shortEvidenceLabel(task.docs || '待补充'),
      evidenceDetail: task.docs || '待补充',
    })),
    evidence: buildEvidenceCards(evidence, metrics),
    risks: risks.length ? risks.map((item) => item.text || String(item)) : ['暂无风险'],
    timeline: buildTimelineCards(timeline),
    members: existingProject?.members?.length ? existingProject.members : ['本地项目'],
  };
}

function mergeProjectSnapshot(existingProject, snapshot) {
  return mapProjectSnapshotToProject(snapshot, 1, existingProject);
}

function buildProjectFromMissingState(sourcePath, index, errorMessage = '') {
  const fallbackProject = buildProjectFromPath(sourcePath, index);
  return {
    ...fallbackProject,
    blocker: errorMessage || fallbackProject.blocker,
  };
}

function buildProjectFromPath(sourcePath, index) {
  const resolvedName = shortLabel(sourcePath) || `未命名项目 ${index}`;
  const alias = buildAlias(resolvedName, sourcePath, index);
  return {
    id: `project-${Date.now()}-${index}`,
    name: resolvedName,
    alias,
    sourcePath,
    sourceType: 'local',
    domain: '未接入状态骨架',
    owner: '本地项目',
    lead: '等待 workflow-bootstrap',
    iteration: '尚未初始化',
    release: '未设置',
    stage: 'bootstrap',
    gateStatus: '待初始化',
    health: '待扫描',
    risk: '中',
    lastUpdated: formatTimestamp(new Date()),
    summary: `${resolvedName} 已接入驾驶舱，但当前路径下还没有 project-state.json。`,
    syncStatus: 'stale',
    syncLabel: errorMessage ? '读取失败' : '未检测到状态文件',
    evidenceTotal: 0,
    metrics: {
      totalTasks: 0,
      doing: 0,
      blocked: 0,
      review: 0,
      done: 0,
      evidenceCoverage: 0,
      releaseGate: '未进入',
    },
    stageStates: buildStageStates('bootstrap'),
    currentTask: '待创建',
    currentFocus: '先执行 workflow-bootstrap',
    blocker: errorMessage || '当前路径下还没有状态骨架，驾驶舱无法读取动态数据。',
    tasks: [],
    evidence: [
      {
        title: '状态骨架',
        value: '缺失',
        detail: '先在项目里生成 .ai/runtime/project-state.json',
      },
    ],
    risks: ['当前项目未完成 workflow 接入，无法显示真实任务、证据和时间线。'],
    timeline: [
      {
        time: formatClock(new Date()),
        label: '接入驾驶舱',
        desc: '路径已记录，但还没有检测到状态文件。',
      },
    ],
    members: ['本地项目'],
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

function getProjectDisplayName(project) {
  if (!project) return '未选择项目';
  return humanizeProjectName(project.name || project.alias || '未命名项目');
}

function humanizeProjectName(value) {
  const raw = String(value || '').trim();
  if (!raw) return '未命名项目';
  if (/[\u4e00-\u9fa5]/.test(raw)) return raw;

  return raw
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part) => {
      if (part.length <= 4) return part.toUpperCase();
      return part.charAt(0).toUpperCase() + part.slice(1);
    })
    .join(' ');
}

function buildSnapshotSummary(name, project, workflow) {
  const language = project.language ? `${project.language} / ` : '';
  const buildTool = project.buildTool || '待补充';
  const gateStatus = workflow.gateStatus || '未同步';
  return `${name} 已接入本地状态快照，当前阶段为 ${stageTitle(normalizeStage(workflow.stage))}，门禁状态 ${gateStatus}，技术栈 ${language}${buildTool}。`;
}

function normalizeStage(stage) {
  if (stage === 'execution' || stage === 'requirement' || stage === 'bootstrap') {
    return stage;
  }
  return 'bootstrap';
}

function normalizeRisk(risk) {
  if (risk === '高' || risk === '中' || risk === '低') return risk;
  if (risk === '异常') return '高';
  if (risk === '观察中') return '中';
  return '低';
}

function normalizeTaskStatus(status) {
  const normalized = String(status || '').toLowerCase();
  if (normalized === 'todo' || normalized === '待处理' || normalized === '待办') return 'todo';
  if (normalized === 'doing' || normalized === '进行中') return 'doing';
  if (normalized === 'review' || normalized === '待审核' || normalized === '待人工审核') return 'review';
  if (normalized === 'blocked' || normalized === '阻塞中') return 'blocked';
  if (normalized === 'done' || normalized === '已完成' || normalized === '已收口') return 'done';
  return 'todo';
}

function normalizeSyncStatus(status) {
  if (status === 'fresh' || status === 'preview') return 'fresh';
  return 'stale';
}

function buildSyncLabel(sync) {
  if (!sync?.lastSyncAt) return sync?.status === 'preview' ? '预览状态' : '未同步';
  return `已同步 ${formatSyncTime(sync.lastSyncAt)}`;
}

function buildEvidenceCards(evidence, metrics) {
  const cards = evidence.slice(0, 3).map((item, index) => ({
    title: item.kind === 'file' ? `证据 ${index + 1}` : item.title || `证据 ${index + 1}`,
    value: shortLabel(item.ref || item.value || '已记录'),
    detail: item.ref || item.detail || '已接入状态快照',
  }));

  if (!cards.length) {
    cards.push({
      title: '证据完整度',
      value: `${metrics.evidenceCoverage || 0}%`,
      detail: '当前状态快照里还没有明细证据。',
    });
  }

  return cards;
}

function shortEvidenceLabel(value) {
  const text = String(value || '').trim();
  if (!text) return '待补充';
  if (text.length <= 32 && !text.includes('/')) return text;
  return shortLabel(text);
}

function buildTimelineCards(timeline) {
  return timeline
    .slice(-5)
    .reverse()
    .map((item) => ({
      time: item.time || '--:--',
      label: item.title || item.stage || '状态更新',
      desc: item.detail || '已同步',
    }));
}

function buildAlias(name, sourcePath, index) {
  const aliasSource = shortLabel(sourcePath) || name || `P${index}`;
  return aliasSource.replace(/[^a-zA-Z0-9]/g, '').slice(0, 4).toUpperCase() || `P${index}`;
}

function formatSyncTime(value) {
  if (!value) return formatTimestamp(new Date());
  const date = typeof value === 'string' ? new Date(value.replace(' ', 'T')) : new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  return formatTimestamp(date);
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

function rankTasks(tasks, currentTaskId) {
  return [...tasks].sort((left, right) => {
    if (left.id === currentTaskId) return -1;
    if (right.id === currentTaskId) return 1;

    const leftWeight = taskStatusWeight(left.status);
    const rightWeight = taskStatusWeight(right.status);
    if (leftWeight !== rightWeight) {
      return leftWeight - rightWeight;
    }

    return String(right.id).localeCompare(String(left.id), 'zh-Hans-CN', { numeric: true });
  });
}

function getTaskWindow(tasks, project) {
  if (!project) return tasks;
  if (project.sourceType !== 'local') return tasks;
  if (tasks.length <= 8) return tasks;
  if (project.stage !== 'execution') return tasks.slice(0, 10);

  const unfinished = tasks.filter((task) => task.status !== 'done');
  if (unfinished.length) {
    return tasks.slice(0, 10);
  }

  return tasks.slice(0, 6);
}

function taskStatusWeight(status) {
  if (status === 'doing') return 0;
  if (status === 'review') return 1;
  if (status === 'blocked') return 2;
  if (status === 'todo') return 3;
  if (status === 'done') return 4;
  return 5;
}

function inferProjectStatus(project) {
  if (!project) return 'todo';
  if (project.metrics?.blocked > 0) return 'blocked';
  if (project.gateStatus === '待人工审核' || project.gateStatus === '待验证') return 'review';
  if (project.metrics?.doing > 0) return 'doing';
  if (project.metrics?.done > 0) return 'done';
  return 'todo';
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

      <h3>{getProjectDisplayName(project)}</h3>
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
  const metaItems = [task.reqId, task.owner, task.priority].filter(Boolean);
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
        {metaItems.map((item) => (
          <span key={item}>{item}</span>
        ))}
      </div>

      <div className="task-flow">
        <div>
          <span>下一步</span>
          <strong>{task.next}</strong>
        </div>
        <div>
          <span>证据</span>
          <strong title={task.evidenceDetail || task.evidence}>{task.evidence}</strong>
        </div>
      </div>

      {task.blocker ? <p className="task-blocker">{task.blocker}</p> : null}
    </article>
  );
}

export default App;
