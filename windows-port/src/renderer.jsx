import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import "./style.css";
function App() {
  const [status, setStatus] = useState(null);
  const [error, setError] = useState("");
  useEffect(() => { window.diPlay.getStatus().then(setStatus).catch((err) => setError(String(err))); }, []);
  return <main className="shell">
    <header className="topbar"><div className="brand-mark">D</div><div><div className="brand-title">DiPlay</div><div className="brand-subtitle">Windows Receiver · 초기 개발 버전</div></div><span className="platform">WINDOWS 11</span></header>
    <section className="hero"><div className="eyebrow">WINDOWS PORT / MILESTONE 01</div><h1>CarPlay 연결 기반을 준비하고 있어요.</h1><p>현재는 데스크톱 앱과 네이티브 백엔드의 기본 골격입니다. USB 장치 검색 기능은 다음 단계에서 구현합니다.</p></section>
    <section className="grid">
      <article className="panel"><div className="panel-label">iPhone USB</div><div className="state"><span className="dot muted" /> 아직 확인 전</div><p>USB 장치 검색 기능이 아직 연결되지 않았습니다.</p></article>
      <article className="panel"><div className="panel-label">Native backend</div><div className="state"><span className="dot muted" /> {status?.backend ?? "확인 중"}</div><p>Rust 백엔드 실행 및 IPC 연결을 준비 중입니다.</p></article>
      <article className="panel"><div className="panel-label">Protocol pipeline</div><div className="state"><span className="dot muted" /> 미구현</div><p>USBMUX → Lockdown → iAP2 → NCM 순서로 단계적으로 구현합니다.</p></article>
    </section>
    <footer className="footer"><span>{status ? status.app + " · " + status.platform : "앱 상태를 불러오는 중"}</span>{error && <span className="error">{error}</span>}<span>CarPlay / MFi 인증은 아직 구현되지 않았습니다.</span></footer>
  </main>;
}
createRoot(document.getElementById("root")).render(<App />);
