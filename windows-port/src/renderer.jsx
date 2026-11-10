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
  const [lockdownResult, setLockdownResult] = useState(null);
  const [queryingLockdown, setQueryingLockdown] = useState(false);
  const [pairingMaterial, setPairingMaterial] = useState(null);
  const [preparingPairing, setPreparingPairing] = useState(false);
  const [pairingPreflight, setPairingPreflight] = useState(null);
  const [checkingPairing, setCheckingPairing] = useState(false);
  const [pairingRequest, setPairingRequest] = useState(null);
  const [requestingPairing, setRequestingPairing] = useState(false);
  const [pairValidation, setPairValidation] = useState(null);
  const [validatingPair, setValidatingPair] = useState(false);
  const [sessionResult, setSessionResult] = useState(null);
  const [startingSession, setStartingSession] = useState(false);
  const [serviceDiscovery, setServiceDiscovery] = useState(null);
  const [discoveringServices, setDiscoveringServices] = useState(false);

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

  const queryLockdown = async () => {
    setQueryingLockdown(true);
    setError("");
    try {
      const result = await window.diPlay.queryLockdownType();
      if (result.status !== "ok") throw new Error(result.message || "Lockdown 연결에 실패했습니다.");
      setLockdownResult(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setQueryingLockdown(false);
    }
  };


  const checkPairingPreflight = async () => {
    setCheckingPairing(true);
    setError("");
    try {
      const result = await window.diPlay.pairingPreflight();
      if (result.status !== "ok") throw new Error(result.message || "Pairing 사전 점검에 실패했습니다.");
      setPairingPreflight(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setCheckingPairing(false);
    }
  };


  const preparePairing = async () => {
    setPreparingPairing(true);
    setError("");
    try {
      const result = await window.diPlay.preparePairing();
      if (result.status !== "ok") throw new Error(result.message || "Pairing 인증서 생성에 실패했습니다.");
      setPairingMaterial(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setPreparingPairing(false);
    }
  };

  const requestPairing = async () => {
    const approved = window.confirm(
      "실제 iPhone Lockdown Pair 요청을 보냅니다. iPhone에 이 컴퓨터를 신뢰할지 묻는 창이 나타날 수 있습니다. 계속할까요?"
    );
    if (!approved) return;
    setRequestingPairing(true);
    setError("");
    try {
      const result = await window.diPlay.requestPairing();
      if (result.status !== "ok") throw new Error(result.message || "Lockdown Pair 요청에 실패했습니다.");
      setPairingRequest(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setRequestingPairing(false);
    }
  };


  const startLockdownSession = async () => {
    setStartingSession(true);
    setError("");
    try {
      const result = await window.diPlay.startLockdownSession();
      if (result.status !== "ok") throw new Error(result.message || "StartSession에 실패했습니다.");
      setSessionResult(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setStartingSession(false);
    }
  };


  const discoverServices = async () => {
    setDiscoveringServices(true);
    setError("");
    try {
      const result = await window.diPlay.discoverLockdownServices();
      if (result.status !== "ok") throw new Error(result.message || "서비스 탐색에 실패했습니다.");
      setServiceDiscovery(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setDiscoveringServices(false);
    }
  };

  const validatePairing = async () => {
    setValidatingPair(true);
    setError("");
    try {
      const result = await window.diPlay.validatePairing();
      if (result.status !== "ok") throw new Error(result.message || "ValidatePair 확인에 실패했습니다.");
      setPairValidation(result.devices);
    } catch (err) {
      setError(err.message || String(err));
    } finally {
      setValidatingPair(false);
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
    <section className="panel scan-panel">
      <div><div className="panel-label">LOCKDOWN SERVICE</div><div className="state">USBMUX Connect → iPhone TCP 62078 → QueryType</div></div>
      <button className="scan-button" onClick={queryLockdown} disabled={queryingLockdown || status?.backend !== "ready"}>{queryingLockdown ? "연결 확인 중..." : "Lockdown 서비스 연결 테스트"}</button>
    </section>

    <section className="panel scan-panel">
      <div><div className="panel-label">LOCKDOWN PAIRING PREFLIGHT</div><div className="state">GetValue → UniqueDeviceID / DeviceName / ProductVersion / DevicePublicKey</div></div>
      <button className="scan-button" onClick={checkPairingPreflight} disabled={checkingPairing || status?.backend !== "ready"}>{checkingPairing ? "사전 점검 중..." : "Pairing 사전 점검"}</button>
    </section>
    {pairingPreflight && <section className="results">
      <div className="results-heading"><h2>Pairing 사전 점검</h2><span>{pairingPreflight.has_device_public_key ? "준비 완료" : "확인 필요"}</span></div>
      <article className="device"><div className="device-info">
        <strong>{pairingPreflight.device_name || "iPhone"} · iOS {pairingPreflight.product_version || "버전 확인 불가"}</strong>
        <span>DeviceID: {pairingPreflight.device_id} · DevicePublicKey: {pairingPreflight.device_public_key_bytes ?? 0} bytes</span>
        <small>UDID: {pairingPreflight.unique_device_id || "응답 없음"}</small>
      </div></article>
    </section>}

    <section className="panel scan-panel">
      <div><div className="panel-label">PAIRING MATERIAL GENERATION</div><div className="state">iPhone 공개 키 기반 인증서 생성 · Windows DPAPI 보호 저장</div></div>
      <button className="scan-button" onClick={preparePairing} disabled={preparingPairing || status?.backend !== "ready"}>{preparingPairing ? "인증서 생성 중..." : "Pairing 인증서 준비"}</button>
    </section>
    <section className="panel scan-panel">
      <div><div className="panel-label">LOCKDOWN PAIR REQUEST</div><div className="state">저장된 PairRecord를 사용해 iPhone에 실제 신뢰 요청 전송</div></div>
      <button className="scan-button" onClick={requestPairing} disabled={requestingPairing || status?.backend !== "ready"}>{requestingPairing ? "Pair 요청 중..." : "iPhone 신뢰 요청 보내기"}</button>
    </section>
    <section className="panel scan-panel">
      <div><div className="panel-label">LOCKDOWN START SESSION</div><div className="state">iOS 7 이상은 ValidatePair 대신 StartSession으로 세션을 시작합니다. TLS가 요청되면 Host 인증서로 핸드셰이크합니다.</div></div>
      <button className="scan-button" onClick={startLockdownSession} disabled={startingSession || status?.backend !== "ready"}>{startingSession ? "세션 시작 중..." : "Lockdown 세션 시작"}</button>
    </section>


    <section className="panel scan-panel">
      <div><div className="panel-label">LOCKDOWN SERVICE DISCOVERY</div><div className="state">TLS 세션 → 제한된 저위험 서비스의 StartService 응답 확인 · 서비스 명령 미실행</div></div>
      <button className="scan-button" onClick={discoverServices} disabled={discoveringServices || status?.backend !== "ready"}>{discoveringServices ? "서비스 탐색 중..." : "Lockdown 서비스 탐색"}</button>
    </section>
    {serviceDiscovery && <section className="results">
      <div className="results-heading"><h2>Lockdown 서비스 탐색 결과</h2><span>{serviceDiscovery.services.filter((item) => item.available).length}개 사용 가능</span></div>
      <article className="device"><div className="device-info">
        <strong>{serviceDiscovery.device_name} · iOS {serviceDiscovery.product_version || "버전 확인 불가"}</strong>
        <span>{serviceDiscovery.product_type || "ProductType 미확인"} · Build {serviceDiscovery.build_version || "미확인"} · TLS {String(serviceDiscovery.tls_established)}</span>
        <small>{serviceDiscovery.message}</small>
      </div></article>
      {serviceDiscovery.services.map((item) => <article className="device" key={item.identifier}>
        <div className="device-icon">SVC</div>
        <div className="device-info">
          <strong>{item.identifier}</strong>
          <span>{item.available ? "StartService 성공 · Port " + item.port + " · SSL " + String(item.ssl_enabled) + " · TLS " + (item.tls_established === true ? "성공" : item.tls_established === false ? "실패" : "미실행") : "사용 불가 또는 시작 거부"}</span>
          {item.error && <small>{item.error}</small>}
        </div>
      </article>)}
    </section>}

    {sessionResult && <section className="results">
      <div className="results-heading"><h2>Lockdown 세션 결과</h2><span>{sessionResult.tls_established ? "TLS 연결 성공" : "세션 연결 성공"}</span></div>
      <article className="device"><div className="device-info">
        <strong>DeviceID: {sessionResult.device_id} · {sessionResult.device_name}</strong>
        <span>{sessionResult.message}</span>
        <small>SessionID: {sessionResult.session_id}</small>
        <small>EnableSessionSSL: {String(sessionResult.ssl_enabled)} · TLS established: {String(sessionResult.tls_established)}</small>
      </div></article>
    </section>}
    {pairingRequest && <section className="results">
      <div className="results-heading"><h2>Lockdown Pair 결과</h2><span>{pairingRequest.pairing_state}</span></div>
      <article className="device"><div className="device-info">
        <strong>DeviceID: {pairingRequest.device_id} · {pairingRequest.unique_device_id}</strong>
        <span>{pairingRequest.message}</span>
        <small>{pairingRequest.response_preview}</small>
      </div></article>
    </section>}
    {pairingMaterial && <section className="results">
      <div className="results-heading"><h2>Pairing 인증서 준비 결과</h2><span>{pairingMaterial.prepared ? "준비 완료" : "실패"}</span></div>
      <article className="device"><div className="device-info">
        <strong>DeviceID: {pairingMaterial.device_id} · {pairingMaterial.unique_device_id}</strong>
        <span>Root: {pairingMaterial.root_certificate_bytes} bytes · Host: {pairingMaterial.host_certificate_bytes} bytes · Device: {pairingMaterial.device_certificate_bytes} bytes</span>
        <small>HostID: {pairingMaterial.host_id}</small>
        <small>SystemBUID: {pairingMaterial.system_buid}</small>
        <small>보호 저장 위치: {pairingMaterial.protected_record_path}</small>
      </div></article>
      <p>PairRecord는 Windows DPAPI로 보호 저장되며, iPhone의 Pair 응답으로 받은 EscrowBag도 함께 보관합니다.</p>
    </section>}
    {lockdownResult && <section className="results">
      <div className="results-heading"><h2>Lockdown 연결 결과</h2><span>{lockdownResult.connected ? "연결 성공" : "연결 실패"}</span></div>
      <article className="device"><div className="device-info">
        <strong>DeviceID: {lockdownResult.device_id} · Port: {lockdownResult.port}</strong>
        <span>Lockdown Type: {lockdownResult.lockdown_type || "응답에 Type 필드 없음"}</span>
        <small>{lockdownResult.response_preview}</small>
      </div></article>
    </section>}
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
