import { useTranslation } from "react-i18next"

export default function ReportPage() {
  const { t } = useTranslation()

  return (
    <div className="p-6 space-y-6">
      <h1 className="text-2xl font-bold">{t("reports")}</h1>
      <p className="text-muted-foreground">{t("no_reports")}</p>
    </div>
  )
}
