import React, { startTransition, useEffect, useState } from 'react';
import { projects as seedProjects, stageMeta, workspace } from './data';
import {
  buildInitialProjects,
  buildProjectFromDraft,
  countTasks,
  createProjectDraft,
  fetchProjectSnapshot,
  getProjectDisplayName,
  getTaskWindow,
  homePageSize,
  inferProjectStatus,
  mergeProjectSnapshot,
  rankTasks,
  stageStateLabel,
  stageTitle,
  statusLabels,
  summarizeWorkspace,
  shortLabel,
} from './lib/cockpit';
import { ComposerModal } from './components/ComposerModal';
import { MetaChip, MetricCard, ProjectTile, SectionTitle, TaskCard } from './components/CockpitUI';

const inspectorTabs = [
  ['summary', '概览'],
  ['evidence', '证据'],
  ['risk', '风险'],
  ['timeline', '时间线'],
  ['team', '团队'],
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

  const activeProject = projects.find((project) => project.id === activeProjectId) ?? projects[0] ?? null;
  const workspaceStats = summarizeWorkspace(projects);
  const pageCount = Math.max(1, Math.ceil(projects.length / homePageSize));
  const currentPage = Math.min(homePage, pageCount - 1);
  const visibleProjects = projects.slice(currentPage * homePageSize, currentPage * homePageSize + homePageSize);
  const isLastPage = currentPage === pageCount - 1;

  const taskCounts = countTasks(activeProject?.tasks ?? []);
  const orderedTasks = activeProject ? rankTasks(activeProject.tasks ?? [], activeProject.currentTask) : [];
  const filteredTasks =
    activeProject && taskFilter === 'all'
      ? orderedTasks
      : (activeProject?.tasks ?? []).filter((task) => task.status === taskFilter);
  const visibleTasks = getTaskWindow(filteredTasks, activeProject).map((task) => ({
    ...task,
    statusLabel: statusLabels[task.status] ?? task.status,
  }));

  useEffect(() => {
    const localProjects = projects.filter((project) => project.sourceType === 'local');
    if (!localProjects.length) return undefined;

    const intervalId = window.setInterval(() => {
      void syncProjects(localProjects);
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
    try {
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
    } finally {
      setSyncing(false);
    }
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

  const projectStatus = inferProjectStatus(activeProject);

  return (
    <div className={`app-shell app-shell--${view}`}>
      <div className="ambient ambient-a" />
      <div className="ambient ambient-b" />

      <header className="topbar panel">
        <div className="topbar__copy">
          <p className="eyebrow">交付控制台</p>
          <h1>{view === 'home' ? workspace.name : getProjectDisplayName(activeProject)}</h1>
          <p className="topbar__subtitle">
            {view === 'home'
              ? workspace.subtitle
              : activeProject?.summary || '当前项目正在同步本地状态。'}
          </p>
          <div className="topbar-ribbon" aria-hidden="true">
            <span>在线</span>
            {stageMeta.map((stage) => (
              <span key={stage.key}>{stage.label} · {stage.short}</span>
            ))}
            <span>同步</span>
          </div>
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
        <main className="workspace workspace--home">
          <section className="panel board-panel">
            <SectionTitle
              title="项目墙"
              action={
                pageCount > 1 ? (
                  <div className="pager">
                    <button
                      className="ghost-button"
                      type="button"
                      onClick={() => setHomePage((page) => Math.max(0, page - 1))}
                      disabled={currentPage === 0}
                    >
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
                ) : null
              }
            />

            <div className="home-grid">
              {visibleProjects.map((project, index) => (
                <ProjectTile key={project.id} project={project} delay={index * 60} onClick={() => openProject(project.id)} />
              ))}

              {isLastPage ? (
                <button className="project-tile project-tile--new" type="button" onClick={() => setComposerOpen(true)}>
                  <span>+</span>
                  <strong>新建项目</strong>
                  <p>接入本地状态文件或先创建一个空壳项目。</p>
                </button>
              ) : null}
            </div>
          </section>
        </main>
      ) : (
        <main className="workspace workspace--project">
          <aside className="panel sidebar-panel">
            <SectionTitle
              title="项目概览"
              action={
                <button className="ghost-button" type="button" onClick={handleRefresh}>
                  {syncing ? '同步中' : '同步状态'}
                </button>
              }
            />

            <div className="project-identity">
              <span className="project-alias">{activeProject?.alias ?? 'NEW'}</span>
              <h2>{getProjectDisplayName(activeProject)}</h2>
              <p>{activeProject?.summary ?? '请选择项目查看详情。'}</p>
            </div>

            <div className="project-chips">
              <MetaChip label="阶段" value={activeProject ? stageTitle(activeProject.stage) : '—'} compact />
              <MetaChip label="门禁" value={activeProject?.gateStatus ?? '—'} compact />
              <MetaChip label="健康" value={activeProject?.health ?? '—'} compact />
              <MetaChip label="同步" value={activeProject?.syncLabel ?? '未同步'} tone={activeProject?.syncStatus === 'stale' ? 'danger' : 'muted'} compact />
            </div>

            <div className="stage-strip">
              {stageMeta.map((stage, index) => {
                const state = activeProject?.stageStates?.[stage.key] ?? 'next';
                return (
                  <article key={stage.key} className={`stage-card stage-${stage.color} state-${state}`}>
                    <div className="stage-card__head">
                      <span>{String(index + 1).padStart(2, '0')}</span>
                      <em>{stageStateLabel(state)}</em>
                    </div>
                    <strong>{stage.title}</strong>
                    <p>{stage.gate}</p>
                    <small>{stage.output}</small>
                  </article>
                );
              })}
            </div>
          </aside>

          <section className="project-main">
          <section className="panel focus-panel">
              <div className="focus-panel__main">
                <div className="focus-panel__headline">
                  <span className="focus-panel__label">焦点</span>
                  <span className={`status-pill status-${projectStatus}`}>
                    {statusLabels[projectStatus] ?? activeProject?.gateStatus ?? '未同步'}
                  </span>
                </div>
                <strong>{activeProject?.currentFocus ?? '等待下一步'}</strong>
                <p>{activeProject?.blocker ?? '暂无阻塞'}</p>
              </div>

              <div className="focus-panel__stats">
                <MetricCard label="当前任务" value={activeProject?.currentTask ?? '未设置'} accent />
                <MetricCard label="证据" value={`${activeProject?.evidenceTotal ?? activeProject?.evidence?.length ?? 0} 条`} />
                <MetricCard label="任务总数" value={activeProject?.tasks?.length ?? 0} />
                <MetricCard
                  label="证据完整度"
                  value={`${activeProject?.metrics?.evidenceCoverage ?? 0}%`}
                  danger={(activeProject?.metrics?.evidenceCoverage ?? 0) < 60}
                />
              </div>
            </section>

            <section className="panel tasks-panel">
              <SectionTitle
                title="任务执行"
                action={
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
                }
              />

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
          </section>

          <aside className="panel inspector-panel">
            <SectionTitle title="项目洞察" />

            <div className="inspector-tabs">
              {inspectorTabs.map(([key, label]) => (
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
                    <MetricCard label="总任务" value={activeProject?.metrics?.totalTasks ?? 0} />
                    <MetricCard label="进行中" value={activeProject?.metrics?.doing ?? 0} />
                    <MetricCard label="待审核" value={activeProject?.metrics?.review ?? 0} />
                    <MetricCard label="阻塞" value={activeProject?.metrics?.blocked ?? 0} danger />
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
                  <div className="inspector-cards">
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
                          {shortLabel(member)}
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
        <ComposerModal
          draft={draft}
          onDraftChange={setDraft}
          error={composerError}
          onClose={() => setComposerOpen(false)}
          onSubmit={handleCreateProject}
        />
      ) : null}
    </div>
  );
}

function formatTimestamp(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  const hour = String(date.getHours()).padStart(2, '0');
  const minute = String(date.getMinutes()).padStart(2, '0');
  return `${year}-${month}-${day} ${hour}:${minute}`;
}

export default App;
