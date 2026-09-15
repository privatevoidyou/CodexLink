import type { DesktopState } from "../../models/topology";
import { useLocale } from "../../i18n/locale";

function displayProjectName(path: string) {
  const normalized = path.replace(/[\\/]+$/, "");
  if (/^[A-Za-z]:$/.test(normalized)) return normalized;
  return normalized.split(/[\\/]/).filter(Boolean).pop() ?? path;
}
import { projectReadinessLabel } from "../../i18n/presentation";

export function ProjectsPanel({
  state,
  onChooseProject,
  onRemoveProject,
}: {
  state: DesktopState;
  onChooseProject: () => void;
  onRemoveProject: (projectPath: string) => void;
}) {
  const { t } = useLocale();
  const projects = state.projects ?? [];
  const projectReady = state.readiness.runtime_ready;
  return (
    <section className="page-section rc5-page" aria-labelledby="projects-title" data-webcodex-page="projects">
      <div className="page-heading-row rc5-heading-row">
        <div>
          <div className="eyebrow">{t("project.eyebrow")}</div>
          <h1 id="projects-title">{t("project.title")}</h1>
          <p className="lede">{t("project.description")}</p>
        </div>
        <button
          className="primary-button"
          onClick={onChooseProject}
          disabled={Boolean(state.current_operation)}
          data-webcodex-action="add-project"
        >
          + {t("project.add")}
        </button>
      </div>

      <div className="project-count-row">
        <div><strong>{projects.length}</strong><span>{t("project.authorized")}</span></div>
        <span className="project-count-hint">{t("project.countHint")}</span>
      </div>

      <div className="project-library-grid">
      {projects.length > 0 ? (
        projects.map((project) => (
          <article className="project-library-card" key={project.path}>
            <div className="project-library-icon" aria-hidden="true"><span className="nav-icon nav-projects" /></div>
            <span className="project-added-badge">
              <i className="status-dot ready" aria-hidden="true" />
              {t("project.added")}
            </span>
            <div className="project-library-copy">
              <div className="project-library-heading">
                <h2>{displayProjectName(project.path)}</h2>
              </div>
              <code>{project.path}</code>
            </div>
            <div className="project-library-access-row">
              <span>{t("project.aiAccess")}</span>
              <strong className={projectReady ? "ready" : "pending"}>
                {projectReady ? t("common.ready") : projectReadinessLabel(state.readiness.project, t)}
              </strong>
            </div>
            <div className="project-library-footer">
              <span>{project.is_git_repository ? t("project.gitDetected") : t("project.regularFolder")}</span>
              <button
                className="secondary-button"
                onClick={() => onRemoveProject(project.path)}
                disabled={Boolean(state.current_operation)}
                data-webcodex-action="remove-project"
              >
                {t("project.remove")}
              </button>
            </div>
          </article>
        ))
      ) : (
        <div className="empty-state project-empty-state">
          <div className="project-library-icon" aria-hidden="true"><span className="nav-icon nav-projects" /></div>
          <strong>{t("project.none")}</strong>
          <span>{t("setup.projectRequired")}</span>
          <button className="primary-button" onClick={onChooseProject} disabled={Boolean(state.current_operation)} data-webcodex-action="add-project">
            + {t("project.add")}
          </button>
        </div>
      )}
      </div>
      <p className="project-library-note">{t("project.runnerScopeNote")}</p>
    </section>
  );
}
