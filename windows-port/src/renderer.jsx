import React, { useCallback, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import "./style.css";

function App() {
  const [status, setStatus] = useState(null);
  const [devices, setDevices] = useState([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [inspection, setInspection] = useState(null);
  const [inspecting, setInspecting] = useState(false);
  const [usbmuxResult, setUsbmuxResult] = useState(null);
  const [probingMux, setProbingMux] = useState(false);

  const refreshStatus = useCallback(() => {
    window.diPlay.getStatus().then(setStatus).catch((err) => setError(String(err)));
  }, []);

  const inspectUsb = async () => {
    setInspecting(true);
    setError("");
    try {
      const result = await window.diPlay.inspectUsb();
      if (result.status !== "ok") throw new Error(result.message || "WinUSB 조회에 실패했습니다.");
      setInspection(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setInspecting(false);
    }
  };

  const probeUsbmux = async () => {
    setProbingMux(true);
    setError("");
    try {
      const result = await window.diPlay.probeUsbmux();
      if (result.status !== "ok") throw new Error(result.message || "USBMUX 요청에 실패했습니다.");
      setUsbmuxResult(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setProbingMux(false);
    }
  };

  const scanUsb = async () => {
    setLoading(true);
    setError("");
    try {
      const result = await window.diPlay.listUsbDevices();
      if (result.status !== "ok") throw new Error(result.message || "USB 검색에 실패했습니다.");
      setDevices(result.devices || []);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setLoading(false);
      refreshStatus();
    }
  };

  useEffect(() => {
    refreshStatus();
    const timer = setInterval(refreshStatus, 1200);
    return () => clearInterval(timer);
  }, [refreshStatus]);

  return <main className="shell">
    <header className="topbar">
      <div className="brand-mark">D</div>
      <div><div className="brand-title">DiPlay</div><div className="brand-subtitle">Windows Receiver · 개발 버전</div></div>
      <span className="platform">WINDOWS 11</span>
    </header>
    <section className="hero">
      <div className="eyebrow">WINDOWS PORT / MILESTONE 01</div>
      <h1>USB 연결 상태를 확인해 보세요.</h1>
      <p>Windows 장치 관리자에 등록된 USB 장치를 읽기 전용으로 검색합니다. 아직 iPhone과 CarPlay 프로토콜을 연결하는 단계는 아닙니다.</p>
    </section>
    <section className="panel scan-panel">
      <div><div className="panel-label">USB DEVICE DISCOVERY</div><div className="state"><span className={"dot " + (status?.backend === "ready" ? "live" : "muted")} /> Rust backend: {status?.backend || "checking"}</div></div>
      <button className="scan-button" onClick={scanUsb} disabled={loading || status?.backend !== "ready"}>{loading ? "검색 중..." : "USB 장치 검색"}</button>
    </section>
    {status?.backendError && <div className="error-box">Rust backend: {status.backendError}</div>}
    {error && <div className="error-box">{error}</div>}
    <section className="panel scan-panel">
      <div><div className="panel-label">APPLE USBMUX INTERFACE</div><div className="state">WinUSB Descriptor / Endpoint 조회 · 읽기 전용</div></div>
      <button className="scan-button" onClick={inspectUsb} disabled={inspecting || status?.backend !== "ready"}>{inspecting ? "조회 중..." : "iPhone 인터페이스 분석"}</button>
    </section>
    <section className="panel scan-panel">
      <div><div className="panel-label">APPLE USBMUX SERVICE</div><div className="state">127.0.0.1:27015 · ListDevices 요청</div></div>
      <button className="scan-button" onClick={probeUsbmux} disabled={probingMux || status?.backend !== "ready"}>{probingMux ? "요청 중..." : "Apple USBMUX 장치 조회"}</button>
    </section>
    {usbmuxResult && <section className="results">
      <div className="results-heading"><h2>USBMUX 응답</h2><span>{usbmuxResult.device_count ?? "확인 불가"}개 장치</span></div>
      <article className="device"><div className="device-info">
        <strong>MessageType: {usbmuxResult.response_message || "응답 필드 없음"}</strong>
        <span>Result: {usbmuxResult.result_number ?? "N/A"} · TCP 연결: {usbmuxResult.connected ? "성공" : "실패"}</span>
        <small>{usbmuxResult.response_preview}</small>
      </div></article>
    </section>}
    {inspection && <section className="results">
      <div className="results-heading"><h2>WinUSB 분석 결과</h2><span>{inspection.interfaces.length}개 인터페이스</span></div>
      {inspection.interfaces.map((item, index) => <article className="device" key={index}>
        <div className="device-icon">IF</div>
        <div className="device-info"><strong>Interface {item.interface_number} · Alternate {item.alternate_setting}</strong><span>Class {item.interface_class} / Subclass {item.interface_subclass} / Protocol {item.interface_protocol} · Endpoint {item.endpoint_count}개</span>
          {item.endpoints.map((ep) => <small key={ep.index}>EP {ep.address} · {ep.direction} · {ep.transfer_type} · Max packet {ep.max_packet_size} · Interval {ep.interval}</small>)}
        </div>
      </article>)}
    </section>}
    <section className="results">
      <div className="results-heading"><h2>검색 결과</h2><span>{devices.length}개 장치</span></div>
      {devices.length === 0 ? <div className="empty">아직 검색 결과가 없습니다. iPhone을 연결한 뒤 USB 장치 검색을 눌러 보세요.</div> :
        <div className="device-list">{devices.map((device, index) => <article className="device" key={device.hardware_ids.join("|") + index}>
          <div className="device-icon">USB</div>
          <div className="device-info"><strong>{device.name}</strong><span>VID_{device.vendor_id} · PID_{device.product_id}{device.interface_number ? " · MI_" + device.interface_number : ""}</span><small>{device.hardware_ids.join(" / ")}</small></div>
        </article>)}</div>}
    </section>
    <footer className="footer"><span>{status ? status.app + " · " + status.platform : "앱 상태를 불러오는 중"}</span><span>검색은 읽기 전용이며 드라이버를 변경하지 않습니다.</span></footer>
  </main>;
}
createRoot(document.getElementById("root")).render(<App />);
