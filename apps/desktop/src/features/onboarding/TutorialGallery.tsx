import { useEffect, useState } from "react";
import tunnelIdGuide from "../../assets/tutorials/tunnel-id.png";
import secretKey1 from "../../assets/tutorials/secret-key-1.png";
import secretKey2 from "../../assets/tutorials/secret-key-2.png";
import appsPlusPro1 from "../../assets/tutorials/apps-plus-pro-1.png";
import appsPlusPro2 from "../../assets/tutorials/apps-plus-pro-2.png";
import appsPlusPro3 from "../../assets/tutorials/apps-plus-pro-3.png";
import appsPlusPro4 from "../../assets/tutorials/apps-plus-pro-4.png";
import appsPlusPro5 from "../../assets/tutorials/apps-plus-pro-5.png";
import appsPlusPro6 from "../../assets/tutorials/apps-plus-pro-6.png";
import appsBusiness1 from "../../assets/tutorials/apps-business-1.png";
import appsBusiness2 from "../../assets/tutorials/apps-business-2.png";
import appsBusiness3 from "../../assets/tutorials/apps-business-3.png";
import appsBusiness4 from "../../assets/tutorials/apps-business-4.png";
import appsBusiness5 from "../../assets/tutorials/apps-business-5.png";
import appsBusiness6 from "../../assets/tutorials/apps-business-6.png";
import howToUse1 from "../../assets/tutorials/how-to-use-1.png";
import howToUse2 from "../../assets/tutorials/how-to-use-2.png";
import howToUse3 from "../../assets/tutorials/how-to-use-3.png";
import { useLocale } from "../../i18n/locale";

const TUNNEL_IMAGES = [tunnelIdGuide] as const;
const SECRET_KEY_IMAGES = [secretKey1, secretKey2] as const;
const PLUS_PRO_IMAGES = [
  appsPlusPro1,
  appsPlusPro2,
  appsPlusPro3,
  appsPlusPro4,
  appsPlusPro5,
  appsPlusPro6,
] as const;
const BUSINESS_IMAGES = [
  appsBusiness1,
  appsBusiness2,
  appsBusiness3,
  appsBusiness4,
  appsBusiness5,
  appsBusiness6,
] as const;
const USAGE_IMAGES = [howToUse1, howToUse2, howToUse3] as const;

function TutorialGallery({ title, images }: { title: string; images: readonly string[] }) {
  const { t } = useLocale();
  const [previewIndex, setPreviewIndex] = useState<number | null>(null);

  useEffect(() => {
    if (previewIndex === null) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPreviewIndex(null);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [previewIndex]);

  const previewLabel = previewIndex === null
    ? ""
    : `${title} · ${t("onboarding.tutorialStep", { step: previewIndex + 1 })}`;

  return (
    <section className="tutorial-group" aria-label={title}>
      <div className="tutorial-group-heading">
        <strong>{title}</strong>
        <span>× {images.length}</span>
      </div>
      <div className="tutorial-thumbnail-grid">
        {images.map((src, index) => {
          const label = `${title} · ${t("onboarding.tutorialStep", { step: index + 1 })}`;
          return (
            <button
              className="tutorial-thumbnail"
              type="button"
              key={src}
              aria-label={label}
              onClick={() => setPreviewIndex(index)}
            >
              <img src={src} alt={label} />
              <span>{t("onboarding.tutorialStep", { step: index + 1 })}</span>
            </button>
          );
        })}
      </div>
      {previewIndex !== null && (
        <div
          className="tutorial-lightbox"
          role="dialog"
          aria-modal="true"
          aria-label={previewLabel}
          onClick={(event) => {
            if (event.target === event.currentTarget) setPreviewIndex(null);
          }}
        >
          <div className="tutorial-lightbox-content">
            <button
              className="tutorial-lightbox-close"
              type="button"
              aria-label={t("onboarding.tutorialClose")}
              onClick={() => setPreviewIndex(null)}
            >
              <span aria-hidden="true">×</span>
            </button>
            <img src={images[previewIndex]} alt={previewLabel} />
            <span className="tutorial-lightbox-caption">{previewLabel}</span>
          </div>
        </div>
      )}
    </section>
  );
}

export function TunnelCredentialTutorials() {
  const { t } = useLocale();
  return (
    <div className="tutorial-gallery-stack">
      <TutorialGallery title={t("onboarding.tutorialTunnelTitle")} images={TUNNEL_IMAGES} />
      <TutorialGallery title={t("onboarding.tutorialKeyTitle")} images={SECRET_KEY_IMAGES} />
    </div>
  );
}

export function ChatGptAppTutorials() {
  const { t } = useLocale();
  return (
    <div className="tutorial-gallery-stack">
      <TutorialGallery title={t("onboarding.tutorialPlusProTitle")} images={PLUS_PRO_IMAGES} />
      <TutorialGallery title={t("onboarding.tutorialBusinessTitle")} images={BUSINESS_IMAGES} />
      <TutorialGallery title={t("onboarding.tutorialUsageTitle")} images={USAGE_IMAGES} />
    </div>
  );
}
