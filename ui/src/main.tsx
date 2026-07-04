import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import NewDownload from "./components/NewDownload";

// A `?nd=<id>` query marks a spawned "New Download" popup window; everything
// else is the main app window.
const intentId = new URLSearchParams(window.location.search).get("nd");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {intentId ? <NewDownload intentId={intentId} /> : <App />}
  </React.StrictMode>,
);
