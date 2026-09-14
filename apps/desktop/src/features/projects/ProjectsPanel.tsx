import type { DesktopState } from "../../models/topology";
import { useLocale } from "../../i18n/locale";
import { projectReadinessLabel } from "../../i18n/presentation";

export function ProjectsPanel({ state, onChooseProject }: { state: DesktopState; onChooseProject: () => void }) {
  const { t } = useLocale();
  const projectName = state.project?.path.split(/[\\/]/).filter(Boolean).pop();
  const projectReady = state.readiness.project === "ready";
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
          data-webcodex-action={state.project ? "change-project" : "add-project"}
        >
          {state.project ? t("project.change") : `+ ${t("project.add")}`}
        </button>
      </div>

      <div className="project-count-row">
        <div><strong>{state.project ? 1 : 0}</strong><span>{t("project.authorized")}</span></div>
        <span className="project-count-hint">{t("project.countHint")}</span>
      </div>

      <div className="project-library-grid">
      {state.project ? (
        <article className="project-library-card" aria-labelledby="configured-project-title">
          <div className="project-library-icon" aria-hidden="true"><span className="nav-icon nav-projects" /></div>
          <div className="project-library-copy">
            <div className="project-library-heading">
              <h2 id="configured-project-title">{projectName}</h2>
              <span className={`project-access-badge ${projectReady ? "ready" : "pending"}`}>
                <i className={`status-dot ${projectReady ? "ready" : "pending"}`} aria-hidden="true" />
                {projectReady ? t("project.aiAccessReady") : projectReadinessLabel(state.readiness.project, t)}
              </span>
            </div>
            <code>{state.project.path}</code>
            <div className="project-library-meta">
              <span>{t("project.allowedRoot")}: {state.project.allowed_root}</span>
              <span>{state.project.is_git_repository ? t("project.gitDetected") : t("project.gitNotRequired")}</span>
            </div>
          </div>
          <button className="secondary-button" onClick={onChooseProject} disabled={Boolean(state.current_operation)}>
            {t("project.change")}
          </button>
        </article>
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
    </section>
  );
}
