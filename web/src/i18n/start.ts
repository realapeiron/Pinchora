import { locale, ready } from "./index";

// These messages must also work when neither catalogue can be downloaded.
const messages = {
  en: ["Loading", "Something went wrong on our side. Please try again.", "Refresh"],
  tr: ["Yükleniyor", "Bizim tarafta bir sorun oluştu. Lütfen yeniden deneyin.", "Yenile"],
} as const;

export async function start(render: () => void, root = document.getElementById("root")!) {
  const [loading, failure, refresh] = messages[locale()];
  document.documentElement.lang = locale();
  const status = document.createElement("p");
  status.setAttribute("role", "status");
  status.textContent = `${loading}…`;
  root.replaceChildren(status);
  try {
    await ready();
  } catch {
    status.setAttribute("role", "alert");
    status.textContent = failure;
    const reload = document.createElement("button");
    reload.type = "button";
    reload.textContent = refresh;
    reload.onclick = () => window.location.reload();
    root.append(reload);
    return;
  }
  root.replaceChildren();
  render();
}
