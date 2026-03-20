import { useTranslation } from "react-i18next"

export default function SettingsPage() {
  const { t } = useTranslation()

  return (
    <div className="p-6 space-y-8">
      <h1 className="text-2xl font-bold">{t("settings")}</h1>

      <section className="space-y-4">
        <h2 className="text-lg font-semibold">{t("language")}</h2>
        <div className="rounded-lg border p-4">
          <p className="text-muted-foreground">Language selector placeholder</p>
        </div>
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-semibold">{t("cache")}</h2>
        <div className="rounded-lg border p-4">
          <p className="text-muted-foreground">Cache settings placeholder</p>
        </div>
      </section>

      <section className="space-y-4">
        <h2 className="text-lg font-semibold">{t("log_level")}</h2>
        <div className="rounded-lg border p-4">
          <p className="text-muted-foreground">Log level selector placeholder</p>
        </div>
      </section>
    </div>
  )
}
