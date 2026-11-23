import { useEffect, useState } from "react";
import { api, onAppNavigate, onSettingsChanged } from "./api";
import { applyTheme } from "./theme";
import { ChatScreen } from "./screens/ChatScreen";
import { PlaceholderScreen } from "./screens/PlaceholderScreen";
import { SettingsScreen } from "./screens/SettingsScreen";
import { NAV, type ScreenId } from "./types";

export function App() {
  const [screen, setScreen] = useState<ScreenId>("chat");
  const [focus, setFocus] = useState("");
  useEffect(() => {
    void api.setUiState(screen, focus);
  }, [screen, focus]);
  useEffect(() => {
    let stop = () => {};
    void onAppNavigate((tab, nextFocus) => {
      setScreen(tab);
      setFocus(nextFocus);
    }).then((unlisten) => {
      stop = unlisten;
    });
    let stopSettings = () => {};
    void onSettingsChanged(() => {
      void api.getSettings().then((settings) => applyTheme(settings.appearance));
    }).then((unlisten) => {
      stopSettings = unlisten;
    });
    return () => {
      stop();
      stopSettings();
    };
  }, []);
  return (
    <div className="app">
      <header className="topnav">
        <div className="brand">
          <span className="mark" aria-hidden="true" />
          <strong>Central LLM Interface</strong>
        </div>
        {NAV.map((item) => (
          <button
            key={item.id}
            type="button"
            className={item.soon ? "nav-btn soon" : "nav-btn"}
            aria-current={screen === item.id ? "page" : undefined}
            onClick={() => setScreen(item.id)}
          >
            {item.label}
          </button>
        ))}
      </header>
      <div className="screen">
        {screen === "chat" ? <ChatScreen /> : null}
        {screen === "create" ? (
          <PlaceholderScreen
            kicker="Later"
            title="Create"
            body="Create is the next milestone. You will import a ComfyUI workflow, and the app will file it by what it outputs. A mesh or .glb workflow lands under 3D."
          />
        ) : null}
        {screen === "benchmark" ? (
          <PlaceholderScreen
            kicker="Coming soon"
            title="Benchmark"
            body="Benchmark stays in the menu so the layout does not move. The suite itself comes later."
          />
        ) : null}
        {screen === "models" ? (
          <PlaceholderScreen
            kicker="Later"
            title="Models"
            body="The model browser comes later. GGUF files you put in the models folder already appear in the chat picker."
          />
        ) : null}
        {screen === "settings" ? <SettingsScreen focus={focus} /> : null}
      </div>
    </div>
  );
}
