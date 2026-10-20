import { useEffect, useId, useRef, useState } from 'react'
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from 'recharts'
import { LayoutDashboard, Sparkles, Images, CalendarDays, Send, BarChart3, Settings, Search, TrendingUp, FileText, Heart, MessageCircle, Plus, Clock3, CircleCheck, MoreHorizontal, WandSparkles, Zap, Upload, HardDrive, Film, Trash2, RefreshCw, X, Pencil, Bot } from 'lucide-react'
import { invokeDesktop, isDesktop, localAssetUrl } from './tauri.js'
import { DEFAULT_LOCAL_BASE_URL, PROVIDER_PRESETS, isDefaultLocalBase } from './providerPresets'

function weekdayLabel(date) {
  const d = date.getDay()
  return d === 0 ? 'CN' : `T${d + 1}`
}

function getGreeting(date = new Date()) {
  const h = date.getHours()
  if (h >= 5 && h < 11) return 'Chào buổi sáng!👋'
  if (h >= 11 && h < 14) return 'Chào buổi trưa!👋'
  if (h >= 14 && h < 18) return 'Chào buổi chiều!👋'
  return 'Chào buổi tối!👋'
}

const nav = [
  ['Tổng quan', LayoutDashboard], ['Kho nội dung', FileText], ['AI Content', Sparkles],
  ['Lịch AI content', Bot], ['Lịch đăng bài', CalendarDays], ['Thư viện media', Images],
  ['Đăng bài', Send], ['Báo cáo', BarChart3],
]

function StatCard({ icon: Icon, iconClass, label, value, delta, sub }) {
  return <div className="stat-card">
    <div className={`stat-icon ${iconClass}`}><Icon size={20}/></div>
    <div className="stat-label">{label}</div>
    <div className="stat-row"><strong>{value}</strong>{delta && <span className="delta"><TrendingUp size={13}/> {delta}</span>}</div>
    <div className="stat-sub">{sub}</div>
  </div>
}

function formatBytes(bytes) {
  if (!bytes) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}`
}

// Unicode code points, so the count matches Rust's chars().count().
// JS String.length counts UTF-16 units and would differ for astral emoji.
const charCount = (s) => [...(s || '')].length
const STYLE_WARN_CHARS = 12000

function MediaLibrary() {
  const [items, setItems] = useState([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const desktop = isDesktop()

  const refresh = async () => {
    if (!desktop) return
    setBusy(true)
    try {
      const stored = await invokeDesktop('list_media')
      const withUrls = await Promise.all(stored.map(async item => ({ ...item, url: await localAssetUrl(item.path) })))
      setItems(withUrls)
      setError('')
    } catch (reason) { setError(String(reason)) } finally { setBusy(false) }
  }

  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { refresh() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const importFiles = async () => {
    setBusy(true)
    try { await invokeDesktop('import_media'); await refresh() }
    catch (reason) { setError(String(reason)); setBusy(false) }
  }

  const remove = async (id) => {
    if (!window.confirm('Xóa media này khỏi kho lưu trữ cục bộ?')) return
    try { await invokeDesktop('delete_media', { id }); await refresh() }
    catch (reason) { setError(String(reason)) }
  }

  return <section className="media-page">
    <div className="page-heading"><div><p>QUẢN LÝ TỆP CỤC BỘ</p><h1>Thư viện media</h1><span>Ảnh và video được sao chép vào vùng dữ liệu riêng của ứng dụng trên máy tính.</span></div><div className="media-actions"><button className="outline" onClick={refresh} disabled={!desktop || busy}><RefreshCw size={16}/> Làm mới</button><button className="primary" onClick={importFiles} disabled={!desktop || busy}><Upload size={17}/> {busy ? 'Đang xử lý...' : 'Nhập media'}</button></div></div>
    {!desktop && <div className="desktop-notice"><HardDrive size={23}/><div><strong>Hãy mở bằng ứng dụng FlowPost AI Desktop</strong><span>Trình duyệt chỉ dùng để xem trước giao diện. Quyền chọn và lưu tệp cục bộ chỉ được bật trong bản Tauri.</span></div></div>}
    {error && <div className="error-box">{error}</div>}
    <div className="media-summary"><div><Images size={18}/><span><b>{items.filter(i => i.mediaType === 'image').length}</b> hình ảnh</span></div><div><Film size={18}/><span><b>{items.filter(i => i.mediaType === 'video').length}</b> video</span></div><div><HardDrive size={18}/><span><b>{formatBytes(items.reduce((sum, item) => sum + item.size, 0))}</b> đã sử dụng</span></div></div>
    {items.length ? <div className="media-grid">{items.map(item => <article className="media-card" key={item.id}><div className="media-preview">{item.mediaType === 'image' ? <img src={item.url} alt={item.name}/> : <><video src={item.url}/><div className="video-badge"><Film size={15}/> VIDEO</div></>}</div><div className="media-info"><div><strong title={item.name}>{item.name}</strong><span>{item.extension.toUpperCase()} • {formatBytes(item.size)}</span></div><button aria-label={`Xóa ${item.name}`} onClick={() => remove(item.id)}><Trash2 size={16}/></button></div></article>)}</div> : <div className="media-empty"><div><Images size={34}/></div><h2>Kho media đang trống</h2><p>Nhập hình ảnh hoặc video từ ổ cứng để dùng khi tạo và đăng bài.</p><button className="primary" onClick={importFiles} disabled={!desktop}><Upload size={17}/> Chọn tệp từ máy tính</button></div>}
  </section>
}

function ProvidersPanel() {
  const desktop = isDesktop()
  const [providers, setProviders] = useState([])
  const [activeId, setActiveId] = useState('')
  const [form, setForm] = useState({ id: '', displayName: '', baseUrl: DEFAULT_LOCAL_BASE_URL, apiKey: '', models: [], defaultModel: '' })
  const [presetKey, setPresetKey] = useState('')
  const [hasApiKey, setHasApiKey] = useState(false)
  const [modelInput, setModelInput] = useState('')
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState('')
  const [err, setErr] = useState('')
  const [mode, setMode] = useState('create')

  const load = async () => {
    if (!desktop) return
    try {
      const list = await invokeDesktop('list_providers')
      setProviders(list)
      if (list.length > 0) {
        let active = list[0]
        try {
          const a = await invokeDesktop('get_active_provider')
          const found = list.find(p => p.id === a.id)
          if (found) active = found
        } catch (_e) { void _e }
        setActiveId(active.id)
        setForm({ id: active.id, displayName: active.displayName || active.id, baseUrl: active.baseUrl, apiKey: '', models: active.models || [], defaultModel: active.defaultModel || '' })
        setHasApiKey(!!active.hasApiKey)
        setMode('edit')
        setPresetKey('')
      } else {
        setMode('create')
        setActiveId('')
      }
    } catch (e) { setErr(String(e)) }
  }

  const selectProvider = (id) => {
    const p = providers.find(x => x.id === id)
    if (!p) return
    setForm({ id: p.id, displayName: p.displayName || p.id, baseUrl: p.baseUrl, apiKey: '', models: p.models || [], defaultModel: p.defaultModel || '' })
    setHasApiKey(!!p.hasApiKey)
    setMode('edit')
    setPresetKey('')
    setErr(''); setMsg('')
  }

  const handleAddNew = () => {
    setForm({ id: '', displayName: '', baseUrl: DEFAULT_LOCAL_BASE_URL, apiKey: '', models: [], defaultModel: '' })
    setHasApiKey(false)
    setMode('create')
    setPresetKey('custom')
    setErr(''); setMsg('')
  }

  const applyPreset = (key) => {
    setPresetKey(key)
    const preset = PROVIDER_PRESETS.find(p => p.key === key)
    if (!preset) return
    setForm(f => {
      const next = { ...f }
      if (!next.id.trim() && preset.id) next.id = preset.id
      if (!next.displayName.trim() && preset.displayName) next.displayName = preset.displayName
      next.baseUrl = preset.baseUrl
      next.models = [...preset.models]
      return next
    })
    setMsg(`Đã áp mẫu ${preset.label}`)
    setErr('')
  }

  const setActive = async (id) => {
    setBusy(true); setErr(''); setMsg('')
    try {
      await invokeDesktop('set_active_provider', { id })
      await load()
      setMsg('Đã đặt provider đang dùng')
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { load() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const handleSave = async () => {
    if (!form.id.trim()) { setErr('Provider ID không được để trống'); return }
    if (!form.baseUrl.trim()) { setErr('Base URL không được để trống'); return }
    if (!isDefaultLocalBase(form.baseUrl) && form.models.length < 1) { setErr('Cần ít nhất 1 model cho provider'); return }
    setBusy(true); setErr(''); setMsg('')
    try {
      if (mode === 'create') {
        const payload = { id: form.id.trim().toLowerCase(), displayName: form.displayName || null, baseUrl: form.baseUrl.trim(), protocol: 'openai-completions', models: form.models, defaultModel: form.defaultModel.trim(), apiKey: form.apiKey || null }
        await invokeDesktop('create_provider', { payload })
        setMsg('Đã tạo provider')
      } else {
        const payload = { displayName: form.displayName || null, baseUrl: form.baseUrl.trim(), protocol: 'openai-completions', models: form.models, defaultModel: form.defaultModel.trim(), apiKey: form.apiKey || null }
        await invokeDesktop('update_provider', { id: form.id, payload })
        setMsg('Đã cập nhật provider')
      }
      setForm(f => ({ ...f, apiKey: '' }))
      await load()
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  const handleFetch = async () => {
    if (!form.id.trim()) { setErr('Chưa có Provider ID'); return }
    if (mode === 'create') { setErr('Hãy lưu provider trước khi fetch models'); return }
    setBusy(true); setErr(''); setMsg('')
    try {
      const models = await invokeDesktop('fetch_provider_models', { id: form.id })
      const changed = form.models.length !== models.length || form.models.some(m => !models.includes(m))
      if (form.models.length && changed) {
        if (!window.confirm(`Thay thế ${form.models.length} model hiện tại bằng ${models.length} model từ provider?`)) return
      }
      setForm(f => ({ ...f, models }))
      setMsg(`Đã thay thế bằng ${models.length} models`)
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  const addModel = () => {
    const m = modelInput.trim()
    if (!m) return
    if (form.models.includes(m)) { setModelInput(''); return }
    setForm(f => ({ ...f, models: [...f.models, m] })); setModelInput('')
  }
  const removeModel = (m) => setForm(f => ({ ...f, models: f.models.filter(x => x !== m) }))
  const currentPreset = PROVIDER_PRESETS.find(p => p.key === presetKey)

  return <div style={{display:'flex', flexDirection:'column', gap:14}}>
    {err && <div className="error-box">{err}</div>}
    {msg && <div className="desktop-notice" style={{background:'#eef7ee', borderColor:'#cde9cd', color:'#2e6b2e'}}><CircleCheck size={18}/><span>{msg}</span></div>}
    {providers.length>0 && <div className="panel" style={{padding:12, display:'flex', flexDirection:'column', gap:8}}>
      <div style={{display:'flex', justifyContent:'space-between', alignItems:'center'}}><strong style={{fontSize:14}}>Danh sách Providers ({providers.length})</strong><button className="outline" onClick={handleAddNew} disabled={!desktop||busy} style={{height:30, fontSize:13}}><Plus size={12}/> Thêm Provider</button></div>
      <div style={{display:'flex', flexDirection:'column', gap:6}}>
        {providers.map(p=> <div key={p.id} onClick={()=>selectProvider(p.id)} style={{display:'flex', justifyContent:'space-between', alignItems:'center', padding:'8px 10px', border: p.id===form.id ? '2px solid #6558d6' : '1px solid #e8e8ed', borderRadius:8, background: p.id===activeId ? '#f0eefc' : '#fff', cursor:'pointer'}}>
          <div style={{display:'flex', alignItems:'center', gap:8}}><strong style={{fontSize:13}}>{p.displayName || p.id}</strong><span style={{width:8, height:8, borderRadius:8, background: p.hasApiKey ? '#4caf50' : '#ccc', display:'inline-block'}} title={p.hasApiKey ? 'Đã lưu API key' : 'Chưa có key'}/><span style={{fontSize:11, color:'#777'}}>{p.id}</span></div>
          <div style={{display:'flex', alignItems:'center', gap:8}}>
            <span style={{fontSize:11, color: p.id===activeId ? '#6558d6' : '#999'}}>{p.id===activeId ? '● active' : ''} {p.id===form.id && mode==='edit' ? '● đang sửa' : ''}</span>
            {p.id!==activeId && <button className="outline" onClick={(e)=>{ e.stopPropagation(); setActive(p.id) }} disabled={!desktop||busy} style={{height:26, fontSize:11}}>Đặt active</button>}
          </div>
        </div>)}
      </div>
    </div>}
    <div className="panel" style={{padding:16, display:'flex', flexDirection:'column', gap:12}}>
      <div style={{display:'flex', justifyContent:'space-between', alignItems:'center'}}>
        <div style={{display:'flex', alignItems:'center', gap:8}}><strong style={{fontSize:14}}>{form.displayName || form.id || 'Custom provider'}</strong><span style={{fontSize:12, background:'#f0eefc', color:'#6558d6', padding:'2px 7px', borderRadius:10, fontWeight:700}}>{mode==='create' ? 'Mới' : 'Đang sửa'}</span><span style={{width:8, height:8, borderRadius:8, background: hasApiKey ? '#4caf50' : '#ccc', display:'inline-block'}}/></div>
        <div style={{display:'flex', gap:6}}>
          <button className="outline" onClick={async()=>{ if(!window.confirm('Xóa provider này?')) return; try{ await invokeDesktop('delete_provider',{id:form.id}); setMsg('Đã xóa'); await load() }catch(e){setErr(String(e))} }} disabled={!desktop || busy || providers.length<=1} title={providers.length<=1 ? 'Không thể xóa provider cuối cùng' : ''} style={{height:32, fontSize:13, color: providers.length<=1 ? '#aaa' : '#b55041', borderColor: providers.length<=1 ? '#eee' : '#f6d4cd'}}>Delete</button>
        </div>
      </div>
      <div style={{height:1, background:'#eee'}}/>
      <div style={{display:'flex', flexDirection:'column', gap:10}}>
        <div>
          <label style={{fontSize:13, fontWeight:700}}>Mẫu provider nhanh</label>
          <div style={{display:'flex', alignItems:'center', gap:10, marginTop:6}}>
            <select value={presetKey} onChange={e=>applyPreset(e.target.value)} disabled={busy} style={{height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', fontSize:14, background:'#fff'}}>
              <option value="">-- Chọn mẫu --</option>
              {PROVIDER_PRESETS.map(p=> <option key={p.key} value={p.key}>{p.label}</option>)}
            </select>
            {currentPreset?.docsUrl && <a href={currentPreset.docsUrl} target="_blank" rel="noreferrer" style={{fontSize:12, color:'#777'}}>Lấy API key</a>}
          </div>
        </div>
        <div>
          <label style={{fontSize:13, fontWeight:700}}>Provider ID</label>
          <input spellCheck={false} value={form.id} onChange={e=>setForm(f=>({...f, id:e.target.value}))} disabled={mode==='edit' || busy} placeholder="acme-gateway" style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', marginTop:6, fontSize:14, background: mode==='edit' ? '#f9fafb' : '#fff'}}/>
          <div style={{fontSize:12, color:'#777', marginTop:4}}>Lowercase identifier, starting with a letter, that uniquely names this provider in requests and as its credential name.</div>
        </div>
        <div>
          <label style={{fontSize:13, fontWeight:700}}>Display name</label>
          <input spellCheck={false} value={form.displayName} onChange={e=>setForm(f=>({...f, displayName:e.target.value}))} disabled={busy} placeholder="Display name" style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', marginTop:6, fontSize:14}}/>
        </div>
        <div>
          <label style={{fontSize:13, fontWeight:700}}>Base URL</label>
          <input spellCheck={false} value={form.baseUrl} onChange={e=>setForm(f=>({...f, baseUrl:e.target.value}))} disabled={busy} placeholder="Full endpoint URL e.g. https://api.example.com/v1/chat/completions" style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', marginTop:6, fontSize:14}}/>
        </div>
        <div>
          <label style={{fontSize:13, fontWeight:700}}>API key</label>
          <input type="password" value={form.apiKey} onChange={e=>setForm(f=>({...f, apiKey:e.target.value}))} disabled={busy} placeholder={hasApiKey ? 'Đã lưu ●●●● — nhập mới để ghi đè' : 'Enter your API key'} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', marginTop:6, fontSize:14}}/>
        </div>
        <div style={{height:1, background:'#eee', margin:'6px 0'}}/>
        <div>
          <div style={{display:'flex', justifyContent:'space-between', alignItems:'center'}}>
            <label style={{fontSize:13, fontWeight:700}}>Models</label>
            <button className="outline" onClick={handleFetch} disabled={!desktop || busy} style={{height:30, fontSize:13}}><RefreshCw size={12}/> Fetch available models</button>
          </div>
          <div style={{border:'1px dashed #e2e1e7', borderRadius:8, padding:12, marginTop:6, textAlign:'center', fontSize:13, color:'#777', background:'#fafafa'}}>
            {form.models.length ? <div style={{display:'flex', flexWrap:'wrap', gap:6, justifyContent:'flex-start'}}>{form.models.map(m=> <span key={m} style={{background:'#fff', border:'1px solid #e2e1e7', borderRadius:16, padding:'4px 10px', fontSize:13, display:'flex', alignItems:'center', gap:6}}>{m} <button onClick={()=>removeModel(m)} style={{border:0, background:'transparent', cursor:'pointer', padding:0}}><X size={12}/></button></span>)}</div> : 'No models will be shown in the selector. Unlisted IDs still be sent directly.'}
          </div>
          <div style={{display:'flex', gap:8, marginTop:8}}>
            <input spellCheck={false} value={modelInput} onChange={e=>setModelInput(e.target.value)} onKeyDown={e=>{ if(e.key==='Enter'){e.preventDefault(); addModel()} }} disabled={busy} placeholder="model-id, ví dụ xoay-vong-worker-web-128k" style={{flex:1, height:32, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', fontSize:14}}/>
            <button className="outline" onClick={addModel} disabled={busy || !modelInput.trim()} style={{height:32, fontSize:13}}>Add model</button>
          </div>
        </div>
        <div>
          <label style={{fontSize:13, fontWeight:700}}>Model mặc định để viết</label>
          <div style={{marginTop:6}}>
            <SuggestInput ariaLabel="Model mặc định để viết" placeholder="Để trống để dùng model đầu tiên trong danh sách" value={form.defaultModel} onChange={v=>setForm(f=>({...f, defaultModel:v}))} options={distinctSortedLabels([...(form.models||[]), ...(form.defaultModel.trim() ? [form.defaultModel] : [])])}/>
          </div>
          <div style={{fontSize:12, color:'#777', marginTop:4}}>Model này được dùng khi AI Content hoặc lịch AI không chọn model riêng.</div>
        </div>
      </div>
      <div style={{display:'flex', justifyContent:'flex-end', gap:8, marginTop:6}}>
        <button className="outline" onClick={()=>{ setErr(''); setMsg(''); load() }} disabled={busy} style={{height:36}}>Cancel</button>
        <button className="primary" onClick={handleSave} disabled={!desktop || busy} style={{height:36}}><WandSparkles size={14}/> {mode==='create' ? 'Tạo provider' : 'Cập nhật provider'}</button>
      </div>
    </div>
  </div>
}

function SettingsPage() {
  const desktop = isDesktop()
  const [fbInput, setFbInput] = useState('')
  const [status, setStatus] = useState({ hasFacebookToken: false })
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState('')
  const [err, setErr] = useState('')
  const [showPublish, setShowPublish] = useState(false)

  const [pages, setPages] = useState([])
  const [selectedPageId, setSelectedPageId] = useState('')

  const refresh = async () => {
    if (!desktop) return
    try {
      const s = await invokeDesktop('credential_status')
      setStatus(s)
    } catch (e) { setErr(String(e)) }
    try {
      const cfg = await invokeDesktop('get_scheduler_config')
      setSelectedPageId(cfg.selectedPageId || '')
    } catch (_e) { void _e }
  }

  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { refresh() }, []) // eslint-disable-line react-hooks/exhaustive-deps
  const [subTab, setSubTab] = useState('Providers')
  const saveFb = async () => {
    if (!fbInput.trim()) { setErr('Vui lòng nhập Facebook Token'); return }
    setBusy(true); setErr(''); setMsg('')
    try { await invokeDesktop('set_facebook_token', { token: fbInput }); setFbInput(''); await refresh(); setMsg('Đã lưu Facebook Token vào kho bảo mật OS') }
    catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }
  const deleteFb = async () => {
    if (!window.confirm('Xóa Facebook Token khỏi kho bảo mật?')) return
    setBusy(true); setErr(''); setMsg('')
    try { await invokeDesktop('delete_facebook_token'); await refresh(); setPages([]); setMsg('Đã xóa Facebook Token') }
    catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }
  const checkFb = async () => {
    if (!fbInput.trim() && !status.hasFacebookToken) { setErr('Vui lòng nhập token hoặc lưu trước khi kiểm tra'); return }
    // For now, require input to validate
    if (!fbInput.trim()) { setErr('Vui lòng dán token vào ô trên để kiểm tra'); return }
    setBusy(true); setErr(''); setMsg('')
    try {
      const me = await invokeDesktop('validate_facebook_token', { token: fbInput })
      setMsg(`Token hợp lệ: ${me.name} (ID: ${me.id})`)
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }
  const loadPages = async () => {
    if (!fbInput.trim()) { setErr('Vui lòng nhập User Token để tải danh sách Trang'); return }
    setBusy(true); setErr(''); setMsg('')
    try {
      const list = await invokeDesktop('list_facebook_pages', { token: fbInput })
      setPages(list)
      setMsg(`Đã tải ${list.length} Trang, token Page đã cache an toàn (không hiện)`)
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }
  return <section className="media-page">
    <div className="page-heading"><div><p>BẢO MẬT CỤC BỘ</p><h1>Cài đặt</h1><span>Token và API key được lưu trong Credential Manager / Keychain của hệ điều hành, không bao giờ lưu trong frontend hay file JSON.</span></div></div>
    {!desktop && <div className="desktop-notice"><HardDrive size={23}/><div><strong>Hãy mở bằng ứng dụng FlowPost AI Desktop</strong><span>Chức năng bảo mật chỉ hoạt động trong bản Tauri.</span></div></div>}
    <div style={{display:'flex', gap:8, marginBottom:14}}>
      <button className={subTab==='Kết nối'?'primary':'outline'} onClick={()=>{ setSubTab('Kết nối'); setErr(''); setMsg('') }} style={{height:32}}><HardDrive size={14}/> Kết nối</button>
      <button className={subTab==='Providers'?'primary':'outline'} onClick={()=>{ setSubTab('Providers'); setErr(''); setMsg('') }} style={{height:32}}><WandSparkles size={14}/> Providers</button>
    </div>
    {subTab==='Kết nối' ? <div className="panel" style={{padding:20, display:'flex', flexDirection:'column', gap:18}}>
{err && <div className="error-box">{err}</div>}
    {msg && <div className="desktop-notice" style={{background:'#eef7ee', borderColor:'#cde9cd', color:'#2e6b2e'}}><CircleCheck size={18}/><span>{msg}</span></div>}
      <div>
        <h2 style={{fontSize:15, margin:'0 0 8px'}}>Facebook Token</h2>
        <p style={{fontSize:13, color:'#777', margin:'0 0 10px'}}>Dùng cho Graph API đăng bài. Token được mã hóa trong OS vault. Frontend chỉ biết trạng thái <b>{status.hasFacebookToken ? '●●●● đã lưu' : 'chưa lưu'}</b>.</p>
        <div style={{display:'flex', gap:8}}>
          <input type="password" placeholder={status.hasFacebookToken ? 'Đã lưu ●●●● — nhập mới để ghi đè' : 'Nhập Facebook User/Page Access Token'} value={fbInput} onChange={e=>setFbInput(e.target.value)} disabled={!desktop || busy} style={{flex:1, height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', fontSize:14}}/>
          <button className="primary" onClick={saveFb} disabled={!desktop || busy} style={{height:36}}><Zap size={14}/> Lưu</button>
          <button className="outline" onClick={checkFb} disabled={!desktop || busy} style={{height:36}}><Search size={14}/> Kiểm tra</button>
          <button className="outline" onClick={loadPages} disabled={!desktop || busy} style={{height:36}}><FileText size={14}/> Tải Trang</button>
          <button className="outline" onClick={()=>setShowPublish(true)} disabled={!desktop || busy || pages.length===0} style={{height:36}}><Send size={14}/> Đăng bài</button>
          <button className="outline" onClick={deleteFb} disabled={!desktop || busy || !status.hasFacebookToken} style={{height:36}}><Trash2 size={14}/> Xóa</button>
        </div>
        {pages.length > 0 && <div className="panel" style={{marginTop:10, padding:10, background:'#f9fafb'}}>
          <div style={{fontSize:13, fontWeight:700, marginBottom:6}}>Danh sách Trang ({pages.length}) — Page Token đã cache, không hiện:</div>
          {pages.map(p => <div key={p.id} style={{display:'flex', justifyContent:'space-between', padding:'6px 8px', background:'#fff', border:'1px solid #eee', borderRadius:6, marginBottom:4}}><span style={{fontSize:13}}><b>{p.name}</b> <span style={{color:'#777'}}>({p.id})</span></span><span style={{fontSize:12, color:'#2e7d32'}}>● token cached</span></div>)}
          <div style={{display:'flex', gap:8, alignItems:'center', marginTop:10}}>
            <label style={{fontSize:13, fontWeight:700}}>Trang mặc định cho Lịch:</label>
            <select value={selectedPageId} onChange={async e=>{ const v=e.target.value; setSelectedPageId(v); if(!v) return; try{ await invokeDesktop('select_scheduler_page', {pageId: v}); setMsg(`Đã chọn Trang mặc định ${v}`)} catch(err){ setErr(String(err)) } }} disabled={!desktop || busy} style={{flex:1, height:32, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', fontSize:13}}>
              <option value="">-- Chưa chọn --</option>
              {pages.map(p=> <option key={p.id} value={p.id}>{p.name} ({p.id})</option>)}
            </select>
            {selectedPageId && <span style={{fontSize:12, color:'#2e7d32'}}>● đã lưu {selectedPageId}</span>}
          </div>
          {!desktop && <div style={{fontSize:12, color:'#777', marginTop:4}}>Chọn Trang mặc định chỉ hoạt động trong Tauri Desktop</div>}
        </div>}
      </div>
      <div style={{height:1, background:'#eee'}}/>
      <div style={{background:'#f7f6fe', border:'1px solid #eceafa', borderRadius:8, padding:12, fontSize:13, color:'#5e58a6'}}>
        <strong style={{display:'flex', alignItems:'center', gap:6}}><HardDrive size={14}/> Lưu trữ: </strong>
        <span>Windows Credential Manager / macOS Keychain / Linux Secret Service — fallback file <code>secure-credentials.json</code> trong AppData (atomic write). Không bao giờ ghi vào <code>media-index.json</code> hay localStorage.</span>
      </div>
    </div> : <ProvidersPanel />}
    {showPublish && <PublishModal onClose={()=>setShowPublish(false)} onSuccess={()=>{}} />}
  </section>
}

function SettingsStatus() {
  const [status, setStatus] = useState({ hasFacebookToken: false, hasAiProviderKey: false })
  const desktop = isDesktop()
  useEffect(() => {
    if (!desktop) return
    const load = async () => {
      try {
        const s = await invokeDesktop('credential_status')
        let hasAi = false
        try { const active = await invokeDesktop('get_active_provider'); hasAi = !!active.hasApiKey } catch (_e) { void _e }
        setStatus({ hasFacebookToken: s.hasFacebookToken, hasAiProviderKey: hasAi })
      } catch (_e) { void _e }
    }
    load()
    const id = setInterval(load, 4000)
    return () => clearInterval(id)
  }, [desktop])
  const hasAny = status.hasFacebookToken || status.hasAiProviderKey
  return <div className="token-box"><div className="token-title"><span><Zap size={14}/> Bảo mật</span><b style={{color: hasAny ? '#2e7d32' : '#d87642'}}>{hasAny ? 'Đã lưu' : 'Chưa lưu'}</b></div><div className="progress"><i style={{width: status.hasFacebookToken && status.hasAiProviderKey ? '100%' : status.hasFacebookToken || status.hasAiProviderKey ? '50%' : '0%', background: hasAny ? '#4caf50' : '#e4a263'}}/></div><p>{status.hasFacebookToken ? 'FB Token ●●●●' : 'FB Token chưa lưu'} • {status.hasAiProviderKey ? 'AI Provider ●●●●' : 'AI Provider chưa lưu'}</p></div>
}

function AiContentPage() {
  const desktop = isDesktop()
  const [prompt, setPrompt] = useState('')
  const [activeProvider, setActiveProvider] = useState(null)
  const [model, setModel] = useState('')
  const [styles, setStyles] = useState([])
  const [selectedStyleId, setSelectedStyleId] = useState('')
  const [styleDraft, setStyleDraft] = useState({ id: '', name: '', content: '', builtin: false })
  const [creatingStyle, setCreatingStyle] = useState(false)
  const [styleBusy, setStyleBusy] = useState(false)
  const [styleModalOpen, setStyleModalOpen] = useState(false)
  const [title, setTitle] = useState('')
  const [body, setBody] = useState('')
  const [busy, setBusy] = useState(false)
  const [msg, setMsg] = useState('')
  const [err, setErr] = useState('')
  const [mediaIds, setMediaIds] = useState([])
  const [mediaOptions, setMediaOptions] = useState([])
  const [showPicker, setShowPicker] = useState(false)
  const [topics, setTopics] = useState([])
  const [selectedTopicId, setSelectedTopicId] = useState('')
  const [freePrompt, setFreePrompt] = useState('')
  const [topicDraft, setTopicDraft] = useState({ id: '', name: '', template: '', lists: [] })
  const [creatingTopic, setCreatingTopic] = useState(false)
  const [topicBusy, setTopicBusy] = useState(false)
  const [topicModalOpen, setTopicModalOpen] = useState(false)
  const [promptUsed, setPromptUsed] = useState('')
  const [styleWarn, setStyleWarn] = useState('')

  const loadMediaOptions = async () => {
    if (!desktop) return
    try {
      const list = await invokeDesktop('list_media')
      const withUrls = await Promise.all(list.map(async item => ({ ...item, url: await localAssetUrl(item.path) })))
      setMediaOptions(withUrls)
    } catch (e) { setErr(String(e)) }
  }

  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { if (showPicker) loadMediaOptions() }, [showPicker]) // eslint-disable-line react-hooks/exhaustive-deps

  const loadStyles = async () => {
    if (!desktop) return
    try {
      const list = await invokeDesktop('list_styles')
      setStyles(list)
      if (!selectedStyleId || !list.some(s => s.id === selectedStyleId)) {
        if (list.length) {
          setSelectedStyleId(list[0].id)
          setStyleDraft({ id: list[0].id, name: list[0].name, content: list[0].content, builtin: !!list[0].builtin })
        }
      }
    } catch (e) { setErr(String(e)) }
  }

  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { loadStyles() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const loadTopics = async () => {
    if (!desktop) return
    try {
      const list = await invokeDesktop('list_topics')
      setTopics(list)
      if (selectedTopicId && !list.some(t => t.id === selectedTopicId)) {
        setSelectedTopicId('')
      }
    } catch (e) { setErr(String(e)) }
  }

  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { loadTopics() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const loadActiveProvider = async () => {
    if (!desktop) return
    try {
      const active = await invokeDesktop('get_active_provider')
      setActiveProvider(active || null)
    } catch (_e) { void _e }
  }

  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { loadActiveProvider() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const handleSelectTopic = (id) => {
    if (id === selectedTopicId) return
    if (!id) {
      setSelectedTopicId('')
      setPrompt(freePrompt)
    } else {
      if (!selectedTopicId) setFreePrompt(prompt)
      const t = topics.find(x => x.id === id)
      setSelectedTopicId(id)
      setPrompt(t ? t.template : '')
    }
    setErr(''); setMsg('')
  }

  const handleSelectStyle = (id) => {
    const s = styles.find(x => x.id === id)
    if (!s) return
    setSelectedStyleId(id)
    setStyleDraft({ id: s.id, name: s.name, content: s.content, builtin: !!s.builtin })
    setCreatingStyle(false)
    setErr(''); setMsg('')
  }

  const applyStyleResult = (res) => {
    setSelectedStyleId(res.id)
    setStyleDraft({ id: res.id, name: res.name, content: res.content, builtin: !!res.builtin })
    setCreatingStyle(false)
  }

  const selectedStyle = styles.find(s => s.id === selectedStyleId) || null
  const stylePreview = ((selectedStyle && selectedStyle.content) || '').replace(/\s+/g, ' ').trim()

  const selectedTopic = topics.find(t => t.id === selectedTopicId) || null
  const topicPreview = selectedTopic ? (selectedTopic.template || '').replace(/\s+/g, ' ').trim() : 'Chế độ Tự do: không dùng danh sách random.'

  const providerModels = (activeProvider && activeProvider.models) || []
  const providerDefault = ((activeProvider && activeProvider.defaultModel) || '').trim()
  const effectiveDefaultModel = providerDefault || providerModels.map(m => String(m || '').trim()).find(Boolean) || ''
  const modelOptions = distinctSortedLabels([...providerModels, ...(providerDefault ? [providerDefault] : [])])
  const modelPlaceholder = effectiveDefaultModel ? `Mặc định: ${effectiveDefaultModel}` : 'Mặc định của provider'

  const startCreateStyle = () => {
    setCreatingStyle(true)
    setStyleDraft({ id: '', name: '', content: '', builtin: false })
    setErr(''); setMsg('')
  }

  const openCreateStyleModal = () => {
    startCreateStyle()
    setErr('')
    setStyleModalOpen(true)
  }

  const openEditStyleModal = () => {
    if (!selectedStyle) return
    setCreatingStyle(false)
    setStyleDraft({ id: selectedStyle.id, name: selectedStyle.name, content: selectedStyle.content, builtin: !!selectedStyle.builtin })
    setErr('')
    setStyleModalOpen(true)
  }

  const closeStyleModal = () => {
    if (creatingStyle) {
      setStyleDraft({ id: '', name: '', content: '', builtin: false })
    } else {
      const s = styles.find(x => x.id === selectedStyleId)
      if (s) setStyleDraft({ id: s.id, name: s.name, content: s.content, builtin: !!s.builtin })
    }
    setErr('')
    setStyleModalOpen(false)
  }

  const saveStyle = async () => {
    setStyleBusy(true); setErr(''); setMsg('')
    try {
      const res = creatingStyle
        ? await invokeDesktop('create_style', { payload: { name: styleDraft.name, content: styleDraft.content } })
        : await invokeDesktop('update_style', { payload: { id: styleDraft.id, name: styleDraft.name, content: styleDraft.content } })
      await loadStyles()
      applyStyleResult(res)
      setMsg(creatingStyle ? 'Đã thêm style mới' : 'Đã cập nhật style')
      setStyleModalOpen(false)
    } catch (e) { setErr(String(e)) } finally { setStyleBusy(false) }
  }

  const duplicateStyle = async () => {
    setStyleBusy(true); setErr(''); setMsg('')
    try {
      const res = await invokeDesktop('duplicate_style', { id: styleDraft.id })
      await loadStyles()
      applyStyleResult(res)
      setMsg('Đã nhân bản style')
    } catch (e) { setErr(String(e)) } finally { setStyleBusy(false) }
  }

  const deleteStyle = async () => {
    if (!window.confirm(`Xóa style "${styleDraft.name}"?`)) return
    setStyleBusy(true); setErr(''); setMsg('')
    try {
      await invokeDesktop('delete_style', { id: styleDraft.id })
      await loadStyles()
      setMsg('Đã xóa style')
      setStyleModalOpen(false)
    } catch (e) { setErr(String(e)) } finally { setStyleBusy(false) }
  }

  const resetStyle = async () => {
    setStyleBusy(true); setErr(''); setMsg('')
    try {
      const res = await invokeDesktop('reset_style', { id: styleDraft.id })
      await loadStyles()
      applyStyleResult(res)
      setMsg('Đã khôi phục style mặc định')
    } catch (e) { setErr(String(e)) } finally { setStyleBusy(false) }
  }

  const openCreateTopicModal = () => {
    setCreatingTopic(true)
    setTopicDraft({ id: '', name: '', template: '', lists: [] })
    setErr(''); setMsg('')
    setTopicModalOpen(true)
  }

  const openEditTopicModal = () => {
    const t = topics.find(x => x.id === selectedTopicId)
    if (!t) return
    setCreatingTopic(false)
    setTopicDraft({
      id: t.id,
      name: t.name,
      template: t.template,
      lists: (t.lists || []).map(l => ({ name: l.name, variantsText: (l.variants || []).join('\n') }))
    })
    setErr('')
    setTopicModalOpen(true)
  }

  const closeTopicModal = () => {
    setErr('')
    setTopicModalOpen(false)
  }

  const saveTopic = async () => {
    setTopicBusy(true); setErr(''); setMsg('')
    try {
      const lists = topicDraft.lists
        .map(l => ({ name: l.name.trim(), variants: l.variantsText.split('\n').map(s => s.trim()).filter(Boolean) }))
        .filter(l => l.name || l.variants.length)
      const payload = { name: topicDraft.name, template: topicDraft.template, lists }
      const res = creatingTopic
        ? await invokeDesktop('create_topic', { payload })
        : await invokeDesktop('update_topic', { payload: { id: topicDraft.id, ...payload } })
      await loadTopics()
      setSelectedTopicId(res.id)
      setPrompt(res.template || '')
      setCreatingTopic(false)
      setMsg(creatingTopic ? 'Đã thêm chủ đề' : 'Đã cập nhật chủ đề')
      setTopicModalOpen(false)
    } catch (e) { setErr(String(e)) } finally { setTopicBusy(false) }
  }

  const deleteTopic = async () => {
    if (!window.confirm(`Xóa chủ đề "${topicDraft.name}"?`)) return
    setTopicBusy(true); setErr(''); setMsg('')
    try {
      await invokeDesktop('delete_topic', { id: topicDraft.id })
      setSelectedTopicId('')
      setFreePrompt(prompt)
      await loadTopics()
      setMsg('Đã xóa chủ đề')
      setTopicModalOpen(false)
    } catch (e) { setErr(String(e)) } finally { setTopicBusy(false) }
  }

  const generate = async (force = false) => {
    if (!prompt.trim()) { setErr('Vui lòng nhập prompt'); return }
    if (!styleDraft.content.trim()) { setErr('Nội dung style không được để trống'); return }
    const styleLen = charCount(styleDraft.content)
    if (!force && styleLen > STYLE_WARN_CHARS) {
      setStyleWarn(`Nội dung style dài ${styleLen} ký tự. Style này được gửi kèm mọi yêu cầu AI nên có thể tốn token và chậm hơn.`)
      return
    }
    setStyleWarn('')
    setBusy(true); setErr(''); setMsg(''); setPromptUsed('')
    try {
      const topicLists = (topics.find(t => t.id === selectedTopicId)?.lists) || []
      const chosenModel = model.trim()
      const payload = { prompt, style: styleDraft.name.trim(), styleContent: styleDraft.content.trim(), topicLists }
      if (chosenModel) payload.model = chosenModel
      const res = await invokeDesktop('generate_content', { payload })
      setTitle(res.title || '')
      setBody(res.body || '')
      setPromptUsed(res.promptUsed || res.prompt || '')
      setMsg('Đã tạo nội dung')
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  const save = async () => {
    if (!title.trim() && !body.trim()) { setErr('Tiêu đề hoặc nội dung không được để trống'); return }
    if (!prompt.trim()) { setErr('Prompt không được để trống'); return }
    setBusy(true); setErr(''); setMsg('')
    try {
      await invokeDesktop('save_content', { payload: { title, body, prompt, style: styleDraft.name.trim(), customStyle: null, mediaIds, topic: (topics.find(t => t.id === selectedTopicId)?.name) || null } })
      setMsg('Đã lưu vào Kho nội dung')
      setPrompt(''); setTitle(''); setBody(''); setMediaIds([]); setFreePrompt(''); setSelectedTopicId(''); setPromptUsed('')
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  const toggleMedia = (id) => setMediaIds(prev => prev.includes(id) ? prev.filter(x => x!==id) : [...prev, id])

  return <section className="media-page">
    <div className="page-heading"><div><p>AI CONTENT</p><h1>Tạo nội dung với AI</h1><span>Nhập prompt, chọn style, tạo nội dung với AI provider đã cấu hình.</span></div></div>
    {!desktop && <div className="desktop-notice"><HardDrive size={23}/><div><strong>Hãy mở bằng ứng dụng FlowPost AI Desktop</strong><span>Chức năng AI chỉ hoạt động trong bản Tauri.</span></div></div>}
    {err && <div className="error-box">{err}</div>}
    {styleWarn && <div className="error-box" style={{background:'#fff3cd', borderColor:'#ffc107', color:'#664d03'}}>
      <span>{styleWarn}</span>
      <div style={{display:'flex', gap:8, marginTop:8}}>
        <button className="btn btn-ghost" onClick={() => setStyleWarn('')}>Hủy</button>
        <button className="btn btn-primary" onClick={() => generate(true)}>Vẫn tạo</button>
      </div>
    </div>}
    {msg && <div className="desktop-notice" style={{background:'#eef7ee', borderColor:'#cde9cd', color:'#2e6b2e'}}><CircleCheck size={18}/><span>{msg}</span></div>}
    <div className="panel" style={{padding:20, display:'flex', flexDirection:'column', gap:16}}>
      <div>
        <div style={{display:'flex', justifyContent:'space-between', alignItems:'center', gap:8, flexWrap:'wrap'}}>
          <label style={{fontSize:14, fontWeight:700}}>Chủ đề</label>
          <div style={{display:'flex', gap:8}}>
            <button className="outline" onClick={openEditTopicModal} disabled={!desktop || busy || topicBusy || !selectedTopicId} style={{height:32}}><Pencil size={14}/> Sửa</button>
            <button className="outline" onClick={openCreateTopicModal} disabled={!desktop || busy || topicBusy} style={{height:32}}><Plus size={14}/> Thêm chủ đề</button>
          </div>
        </div>
        <select value={selectedTopicId} onChange={e=>handleSelectTopic(e.target.value)} disabled={!desktop || busy || topicBusy} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:6}}>
          <option value="">Tự do (nhập prompt trực tiếp)</option>
          {topics.map(t => <option key={t.id} value={t.id}>{t.name}</option>)}
        </select>
        <div style={{display:'flex', alignItems:'center', gap:6, marginTop:6, color:'#6b6b76', fontSize:13, overflow:'hidden', whiteSpace:'nowrap', textOverflow:'ellipsis'}}><FileText size={14}/><span style={{overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap'}}>{topicPreview}</span></div>
        <div style={{fontSize:12, color:'#999', marginTop:4}}>Chỉnh sửa prompt bên dưới không tự lưu vào chủ đề; dùng nút Sửa để cập nhật chủ đề.</div>
      </div>
      <div>
        <label style={{fontSize:14, fontWeight:700}}>Prompt *</label>
        <textarea spellCheck={false} placeholder="VD: Bí quyết năng lượng buổi sáng cho dân văn phòng..." value={prompt} onChange={e=>setPrompt(e.target.value)} disabled={!desktop || busy} style={{width:'100%', minHeight:90, border:'1px solid #e2e1e7', borderRadius:8, padding:12, fontSize:14, marginTop:6, resize:'vertical'}}/>
        <div style={{fontSize:12, color:'#999', textAlign:'right'}}>{prompt.length} ký tự</div>
        {promptUsed && promptUsed !== prompt && <div style={{fontSize:12, color:'#999', overflow:'hidden', whiteSpace:'nowrap', textOverflow:'ellipsis'}}>Đã dùng: {promptUsed}</div>}
      </div>
      <div style={{display:'grid', gridTemplateColumns:'1fr 1fr 160px', gap:12}}>
        <div>
          <label style={{fontSize:14, fontWeight:700}}>Style</label>
          <select value={selectedStyleId} onChange={e=>handleSelectStyle(e.target.value)} disabled={!desktop || busy || styleBusy} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:6}}>
            {styles.length ? styles.map(s => <option key={s.id} value={s.id}>{s.name}</option>) : <option value="">Mở app desktop để tải style</option>}
          </select>
          {!desktop && <div style={{fontSize:12, color:'#999', marginTop:4}}>Mở app desktop để tải style</div>}
        </div>
        <div>
          <label style={{fontSize:14, fontWeight:700}}>Model</label>
          <div style={{marginTop:6}}>
            <SuggestInput ariaLabel="Model" placeholder={modelPlaceholder} value={model} onChange={setModel} options={modelOptions}/>
          </div>
          <div style={{fontSize:12, color:'#999', marginTop:4}}>Để trống để dùng model mặc định của provider.</div>
        </div>
        <div style={{display:'flex', alignItems:'flex-end'}}>
          <button className="primary" onClick={() => generate()} disabled={!desktop || busy || !prompt.trim()} style={{width:'100%', height:36, justifyContent:'center'}}><Sparkles size={16}/> {busy ? 'Đang tạo...' : 'Tạo nội dung'}</button>
        </div>
      </div>
      <div style={{border:'1px solid #e8e8ed', borderRadius:10, padding:12, display:'flex', flexDirection:'column', gap:4, background:'#fafaff'}}>
        <div style={{display:'flex', justifyContent:'space-between', alignItems:'center', gap:8, flexWrap:'wrap'}}>
          <div style={{display:'flex', alignItems:'center', gap:8}}>
            <span style={{fontSize:13, fontWeight:700}}>Cấu hình style</span>
            <span style={{fontSize:11, fontWeight:700, padding:'2px 8px', borderRadius:999, background: selectedStyle && selectedStyle.builtin ? '#efeeff' : '#eef7ee', color: selectedStyle && selectedStyle.builtin ? '#6558d6' : '#2e6b2e'}}>{selectedStyle && selectedStyle.builtin ? 'Mặc định' : 'Tùy chỉnh'}</span>
          </div>
          <div style={{display:'flex', gap:8}}>
            <button className="outline" onClick={openEditStyleModal} disabled={!desktop || busy || styleBusy || !selectedStyleId} style={{height:32}}><Pencil size={14}/> Sửa</button>
            <button className="outline" onClick={openCreateStyleModal} disabled={!desktop || busy || styleBusy} style={{height:32}}><Plus size={14}/> Thêm style</button>
          </div>
        </div>
        <div style={{display:'flex', alignItems:'center', gap:6, marginTop:6, color:'#6b6b76', fontSize:13, overflow:'hidden', whiteSpace:'nowrap', textOverflow:'ellipsis'}}><FileText size={14}/><span style={{overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap'}}>{stylePreview || 'Chưa có nội dung style'}</span></div>
      </div>
      <div style={{height:1, background:'#eee'}}/>
      <div>
        <label style={{fontSize:14, fontWeight:700}}>Tiêu đề</label>
        <input spellCheck={false} placeholder="Tiêu đề sẽ hiện sau khi Tạo..." value={title} onChange={e=>setTitle(e.target.value)} disabled={busy} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', marginTop:6, fontSize:14}}/>
      </div>
      <div>
        <label style={{fontSize:14, fontWeight:700}}>Nội dung</label>
        <textarea spellCheck={false} placeholder="Nội dung sẽ hiện sau khi Tạo — bạn có thể chỉnh sửa..." value={body} onChange={e=>setBody(e.target.value)} disabled={busy} style={{width:'100%', minHeight:160, border:'1px solid #e2e1e7', borderRadius:8, padding:12, marginTop:6, fontSize:14, resize:'vertical'}}/>
        <div style={{fontSize:12, color:'#999', textAlign:'right'}}>{charCount(body)} ký tự</div>
      </div>
      <div>
        <div style={{display:'flex', justifyContent:'space-between', alignItems:'center'}}>
          <label style={{fontSize:14, fontWeight:700}}>Gắn media ({mediaIds.length})</label>
          <button className="outline" onClick={()=>setShowPicker(!showPicker)} disabled={!desktop} style={{height:32}}><Images size={14}/> {showPicker ? 'Ẩn' : 'Chọn'} media</button>
        </div>
        {showPicker && <div className="media-grid" style={{marginTop:10, gridTemplateColumns:'repeat(3,1fr)'}}>{mediaOptions.length ? mediaOptions.map(m => <article key={m.id} className="media-card" style={{border: mediaIds.includes(m.id) ? '2px solid #6558d6' : '1px solid #e8e8ed', cursor:'pointer'}} onClick={()=>toggleMedia(m.id)}><div className="media-preview" style={{aspectRatio:1}}>{m.mediaType==='image'?<img src={m.url} alt={m.name}/>:<><video src={m.url}/><div className="video-badge"><Film size={12}/> VIDEO</div></>}</div><div className="media-info" style={{padding:8}}><strong style={{fontSize:13}}>{m.name}</strong><span style={{fontSize:12}}>{mediaIds.includes(m.id) ? '✓ Đã chọn' : m.extension.toUpperCase()}</span></div></article>) : <span style={{fontSize:13, color:'#777'}}>Thư viện trống — hãy nhập media trước</span>}</div>}
        {mediaIds.length>0 && <div style={{fontSize:13, color:'#6558d6', marginTop:6}}>Đã chọn {mediaIds.length} media</div>}
      </div>
      <div style={{display:'flex', gap:8, justifyContent:'flex-end'}}>
        <button className="outline" onClick={()=>{setPrompt(''); setTitle(''); setBody(''); setMediaIds([]); setFreePrompt(''); setSelectedTopicId(''); setPromptUsed(''); setMsg(''); setErr(''); setStyleWarn('')}} disabled={busy} style={{height:36}}>Xóa form</button>
        <button className="primary" onClick={save} disabled={!desktop || busy} style={{height:36}}><FileText size={16}/> Lưu vào kho</button>
      </div>
    </div>
    {styleModalOpen && (
      <div onClick={e=>{ if (e.target === e.currentTarget) closeStyleModal() }} style={{position:'fixed', inset:0, background:'#0006', display:'grid', placeItems:'center', zIndex:50}}>
        <div role="dialog" aria-modal="true" aria-label={creatingStyle ? 'Thêm style mới' : 'Sửa style'} tabIndex={-1} className="panel" onKeyDown={e=>{ if (e.key === 'Escape') closeStyleModal() }} style={{width:'min(560px, 92vw)', maxHeight:'86vh', overflow:'auto', background:'#fff', borderRadius:12, padding:16, display:'grid', gap:10}}>
          <div style={{display:'flex', justifyContent:'space-between', alignItems:'center', gap:8}}>
            <label style={{fontSize:14, fontWeight:700}}>{creatingStyle ? 'Thêm style mới' : 'Sửa style'}</label>
            <div style={{display:'flex', alignItems:'center', gap:8}}>
              {!creatingStyle && <span style={{fontSize:11, fontWeight:700, padding:'2px 8px', borderRadius:999, background: styleDraft.builtin ? '#efeeff' : '#eef7ee', color: styleDraft.builtin ? '#6558d6' : '#2e6b2e'}}>{styleDraft.builtin ? 'Mặc định' : 'Tùy chỉnh'}</span>}
              <button className="outline" onClick={closeStyleModal} style={{height:32}}>Đóng</button>
            </div>
          </div>
          <div>
            <label style={{fontSize:13, fontWeight:600}}>Tên style</label>
            <input spellCheck={false} autoFocus value={styleDraft.name} onChange={e=>setStyleDraft(d=>({...d, name:e.target.value}))} maxLength={60} disabled={busy || styleBusy} placeholder="VD: Hài hước GenZ" style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', marginTop:6, fontSize:14}}/>
          </div>
          <div>
            <label style={{fontSize:13, fontWeight:600}}>Nội dung style</label>
            <textarea spellCheck={false} value={styleDraft.content} onChange={e=>setStyleDraft(d=>({...d, content:e.target.value}))} rows={12} disabled={busy || styleBusy} placeholder="Mô tả đầy đủ giọng văn và cấu trúc bài viết cho style này..." style={{width:'100%', minHeight:140, border:'1px solid #e2e1e7', borderRadius:8, padding:12, marginTop:6, fontSize:14, resize:'vertical', fontFamily:'inherit'}}/>
            <div style={{fontSize:12, color:'#999', textAlign:'right'}}>{charCount(styleDraft.content)} ký tự</div>
            {charCount(styleDraft.content) > STYLE_WARN_CHARS && <div className="error-box" style={{background:'#fff3cd', borderColor:'#ffc107', color:'#664d03'}}>Nội dung style đang dài ({charCount(styleDraft.content)} ký tự). Style được gửi kèm mọi yêu cầu AI nên có thể làm tăng chi phí và độ trễ.</div>}
          </div>
          {err && <div className="error-box">{err}</div>}
          {msg && <div className="desktop-notice" style={{background:'#eef7ee', borderColor:'#cde9cd', color:'#2e6b2e'}}><CircleCheck size={18}/><span>{msg}</span></div>}
          <div style={{display:'flex', gap:8, flexWrap:'wrap', justifyContent:'flex-end'}}>
            <button className="primary" onClick={saveStyle} disabled={busy || styleBusy} style={{height:32}}><FileText size={14}/> Lưu thay đổi</button>
            <button className="outline" onClick={duplicateStyle} disabled={creatingStyle || !styleDraft.id || styleBusy} style={{height:32}}>Nhân bản</button>
            <button className="outline" onClick={resetStyle} disabled={!styleDraft.builtin || styleBusy} style={{height:32}}>Khôi phục mặc định</button>
            <button className="outline" onClick={deleteStyle} disabled={styleDraft.builtin || !styleDraft.id || styleBusy} style={{height:32, color:'#b55041'}}>Xóa</button>
            <button className="outline" onClick={closeStyleModal} style={{height:32}}>Đóng</button>
          </div>
        </div>
      </div>
    )}
    {topicModalOpen && (
      <div onClick={e=>{ if (e.target === e.currentTarget) closeTopicModal() }} style={{position:'fixed', inset:0, background:'#0006', display:'grid', placeItems:'center', zIndex:50}}>
        <div role="dialog" aria-modal="true" aria-label={creatingTopic ? 'Thêm chủ đề' : 'Sửa chủ đề'} tabIndex={-1} className="panel" onKeyDown={e=>{ if (e.key === 'Escape') closeTopicModal() }} style={{width:'min(640px, 92vw)', maxHeight:'86vh', overflow:'auto', background:'#fff', borderRadius:12, padding:16, display:'grid', gap:10}}>
          <div style={{display:'flex', justifyContent:'space-between', alignItems:'center', gap:8}}>
            <label style={{fontSize:14, fontWeight:700}}>{creatingTopic ? 'Thêm chủ đề' : 'Sửa chủ đề'}</label>
            <button className="outline" onClick={closeTopicModal} style={{height:32}}>Đóng</button>
          </div>
          <div>
            <label style={{fontSize:13, fontWeight:600}}>Tên chủ đề</label>
            <input spellCheck={false} autoFocus value={topicDraft.name} onChange={e=>setTopicDraft(d=>({...d, name:e.target.value}))} maxLength={60} disabled={busy || topicBusy} placeholder="VD: Nhân quả trong phật giáo" style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', marginTop:6, fontSize:14}}/>
          </div>
          <div>
            <label style={{fontSize:13, fontWeight:600}}>Prompt mẫu</label>
            <textarea spellCheck={false} value={topicDraft.template} onChange={e=>setTopicDraft(d=>({...d, template:e.target.value}))} rows={8} disabled={busy || topicBusy} placeholder="VD: viết 1 đoạn văn theo phong cách [phong cách] về chủ đề Nhân quả trong phật giáo" style={{width:'100%', minHeight:120, border:'1px solid #e2e1e7', borderRadius:8, padding:12, marginTop:6, fontSize:14, resize:'vertical', fontFamily:'inherit'}}/>
            <div style={{fontSize:12, color:'#999', textAlign:'right'}}>{topicDraft.template.length} ký tự</div>
          </div>
          <div>
            <label style={{fontSize:13, fontWeight:600}}>Danh sách từ/cụm từ</label>
            <div style={{fontSize:12, color:'#999', marginTop:4}}>Dùng [tên danh sách] trong prompt mẫu để random.</div>
            <div style={{display:'grid', gap:10, marginTop:8}}>
              {topicDraft.lists.map((l, i) => (
                <div key={i} style={{border:'1px solid #e8e8ed', borderRadius:10, padding:10, display:'grid', gap:8}}>
                  <div style={{display:'flex', gap:8, alignItems:'center'}}>
                    <input spellCheck={false} value={l.name} onChange={e=>setTopicDraft(d=>({...d, lists: d.lists.map((x,j)=>j===i?{...x,name:e.target.value}:x)}))} maxLength={60} placeholder="VD: phong cách" style={{flex:1, height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 12px', fontSize:14}}/>
                    <button className="outline" onClick={()=>setTopicDraft(d=>({...d, lists: d.lists.filter((_,j)=>j!==i)}))} style={{height:32, color:'#b55041'}}>Xóa</button>
                  </div>
                  <textarea spellCheck={false} value={l.variantsText} onChange={e=>setTopicDraft(d=>({...d, lists: d.lists.map((x,j)=>j===i?{...x,variantsText:e.target.value}:x)}))} rows={4} placeholder="Mỗi dòng 1 cụm từ" style={{width:'100%', border:'1px solid #e2e1e7', borderRadius:8, padding:12, fontSize:14, resize:'vertical', fontFamily:'inherit'}}/>
                </div>
              ))}
            </div>
            <button className="outline" onClick={()=>setTopicDraft(d=>({...d, lists:[...d.lists, {name:'', variantsText:''}]}))} style={{height:32, marginTop:8}}><Plus size={14}/> Thêm danh sách</button>
          </div>
          {err && <div className="error-box">{err}</div>}
          {msg && <div className="desktop-notice" style={{background:'#eef7ee', borderColor:'#cde9cd', color:'#2e6b2e'}}><CircleCheck size={18}/><span>{msg}</span></div>}
          <div style={{display:'flex', gap:8, flexWrap:'wrap', justifyContent:'flex-end'}}>
            <button className="primary" onClick={saveTopic} disabled={busy || topicBusy} style={{height:32}}><FileText size={14}/> Lưu thay đổi</button>
            <button className="outline" onClick={deleteTopic} disabled={creatingTopic || !topicDraft.id || topicBusy} style={{height:32, color:'#b55041'}}>Xóa</button>
            <button className="outline" onClick={closeTopicModal} style={{height:32}}>Đóng</button>
          </div>
        </div>
      </div>
    )}
  </section>
}

function ScheduleModal({ contentId, onClose, onCreated }) {
  const desktop = isDesktop()
  const [scheduledAt, setScheduledAt] = useState('')
  const [platform, setPlatform] = useState('Sống Tích Cực')
  const [pageId, setPageId] = useState('')
  const [pageOptions, setPageOptions] = useState([])
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState('')
  const [warn, setWarn] = useState('')

  useEffect(() => {
    if (!desktop) return
    const load = async () => {
      try {
        const cfg = await invokeDesktop('get_scheduler_config')
        if (cfg.selectedPageId) setPageId(cfg.selectedPageId)
      } catch (_e) { void _e }
      try {
        const list = await invokeDesktop('list_facebook_pages', { token: '' })
        setPageOptions(list)
        if (list.length && !pageId) {
          try {
            const cfg2 = await invokeDesktop('get_scheduler_config')
            if (!cfg2.selectedPageId) setPageId(list[0].id)
          } catch (_e2) { void _e2; setPageId(list[0].id) }
        }
      } catch (_e) { void _e }
    }
    load()
  }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const checkConflict = async () => {
    if (!scheduledAt || !platform) return ''
    try {
      const list = await invokeDesktop('list_schedule')
      const target = new Date(scheduledAt).getTime()
      const conflict = list.find(s => s.platform === platform && s.status === 'Scheduled' && Math.abs(new Date(s.scheduledAt).getTime() - target) < 30*60*1000)
      if (conflict) return `Cảnh báo: Trùng lịch với "${conflict.platform}" lúc ${new Date(conflict.scheduledAt).toLocaleString('vi-VN')} (cách <30 phút). Bạn có thể vẫn đăng trùng nếu chấp nhận.`
      return ''
    } catch { return '' }
  }

  const handleSchedule = async (ignoreWarn) => {
    if (!contentId) { setErr('Thiếu contentId'); return }
    if (!scheduledAt) { setErr('Vui lòng chọn thời gian'); return }
    if (!pageId.trim()) { setErr('Vui lòng chọn Trang đích (Page ID)'); return }
    if (!ignoreWarn) {
      const w = await checkConflict()
      if (w) { setWarn(w); return }
    }
    setBusy(true); setErr(''); setWarn('')
    try {
      const iso = new Date(scheduledAt).toISOString()
      await invokeDesktop('create_schedule', { payload: { contentId, scheduledAt: iso, platform, pageId, pages: [pageId] } })
      onCreated && onCreated()
      onClose()
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  return <div style={{position:'fixed', inset:0, background:'#0006', display:'grid', placeItems:'center', zIndex:50}} onClick={onClose}>
    <div className="panel" style={{width:420, padding:18, display:'flex', flexDirection:'column', gap:12}} onClick={e=>e.stopPropagation()}>
      <h2 style={{margin:0, fontSize:17}}>Lên lịch đăng</h2>
      <div>
        <label style={{fontSize:14, fontWeight:700}}>Thời gian</label>
        <input type="datetime-local" value={scheduledAt} onChange={e=>{setScheduledAt(e.target.value); setWarn('')}} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:6}}/>
        <div style={{fontSize:12, color:'#888', marginTop:4}}>Phải trong tương lai, tối thiểu 5 phút.</div>
      </div>
      <div>
        <label style={{fontSize:14, fontWeight:700}}>Trang đích *</label>
        {pageOptions.length ? <select value={pageId} onChange={e=>setPageId(e.target.value)} disabled={busy} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:6}}>
          <option value="">-- Chọn Trang --</option>
          {pageOptions.map(p=> <option key={p.id} value={p.id}>{p.name} ({p.id})</option>)}
        </select> : <div style={{fontSize:13, color:'#777', marginTop:6, padding:8, background:'#fafafa', borderRadius:6}}>Chưa có Trang — hãy vào Cài đặt → Kết nối → Tải Trang{!desktop && ' (chỉ Desktop)'}</div>}
        {!desktop && <div style={{fontSize:12, color:'#777', marginTop:4}}>Chọn Trang chỉ hoạt động trong Tauri Desktop</div>}
      </div>
      <div>
        <label style={{fontSize:14, fontWeight:700}}>Nền tảng (label hiển thị)</label>
        <select value={platform} onChange={e=>setPlatform(e.target.value)} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:6}}>
          <option>Sống Tích Cực</option><option>Daily Motivation</option><option>Chill Mỗi Ngày</option>
        </select>
        <div style={{fontSize:12, color:'#777', marginTop:4}}>Dùng để lọc Calendar, không ảnh hưởng Page ID đăng bài.</div>
      </div>
      {warn && <div className="error-box" style={{background:'#fff3cd', borderColor:'#ffc107', color:'#664d03'}}>{warn}<div style={{marginTop:8, display:'flex', gap:8}}><button className="outline" onClick={()=>setWarn('')} style={{height:30}}>Hủy</button><button className="primary" onClick={()=>handleSchedule(true)} style={{height:30}}>Vẫn đăng trùng</button></div></div>}
      {err && <div className="error-box">{err}</div>}
      <div style={{display:'flex', justifyContent:'flex-end', gap:8}}>
        <button className="outline" onClick={onClose} disabled={busy} style={{height:36}}>Đóng</button>
        <button className="primary" onClick={()=>handleSchedule(false)} disabled={busy} style={{height:36}}><CalendarDays size={14}/> Xác nhận</button>
      </div>
    </div>
  </div>
}

function PublishModal({ onClose, onSuccess }) {
  const desktop = isDesktop()
  const [pages, setPages] = useState([])
  const [pageId, setPageId] = useState('')
  const [message, setMessage] = useState('')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState('')
  const [msg, setMsg] = useState('')
  const [mediaItems, setMediaItems] = useState([])
  const [selectedMediaId, setSelectedMediaId] = useState(null)
  const [showPicker, setShowPicker] = useState(false)

  // Load pages when modal opens (use empty token → backend lấy từ keyring)
  useEffect(() => {
    invokeDesktop('list_facebook_pages', { token: '' })
      .then(setPages)
      .catch(e => setErr(String(e)))
  }, [])

  const loadMedia = async () => {
    if (!desktop) return
    try {
      const list = await invokeDesktop('list_media')
      const withUrls = await Promise.all(list.map(async item => ({ ...item, url: await localAssetUrl(item.path) })))
      setMediaItems(withUrls)
    } catch (e) { setErr(String(e)) }
  }

  // eslint-disable-next-line react-hooks/set-state-in-effect, react-hooks/exhaustive-deps
  useEffect(() => { if (showPicker) loadMedia() }, [showPicker])

  const handlePublish = async () => {
    if (!pageId || !message.trim()) return
    setBusy(true); setErr(''); setMsg('')
    try {
      let imagePath = null
      if (selectedMediaId) {
        const found = mediaItems.find(m => m.id === selectedMediaId)
        if (found) imagePath = found.path
      }
      const res = await invokeDesktop('publish_content', { page_id: pageId, message, image_path: imagePath })
      setMsg(`Đã đăng thành công (post ID: ${res.id || 'unknown'})`)
      onSuccess && onSuccess()
      setTimeout(onClose, 1500)
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  return (
    <div style={{position:'fixed', inset:0, background:'#0008', display:'grid', placeItems:'center', zIndex:60}} onClick={onClose}>
      <div className="panel" style={{width:440, padding:24, display:'flex', flexDirection:'column', gap:16}} onClick={e=>e.stopPropagation()}>
        <h2 style={{margin:0, fontSize:19}}>Đăng bài ngay</h2>
        {pages.length === 0 && <p style={{fontSize:14, color:'#666'}}>Chưa có Trang nào. Hãy bấm "Tải Trang" ở Cài đặt để tải danh sách.</p>}
        <select value={pageId} onChange={e=>setPageId(e.target.value)} disabled={busy}>
          <option value="">-- Chọn Trang --</option>
          {pages.map(p => <option key={p.id} value={p.id}>{p.name} ({p.id})</option>)}
        </select>
        <textarea spellCheck={false} value={message} onChange={e=>setMessage(e.target.value)} placeholder="Nội dung bài đăng..." disabled={busy} style={{width:'100%', minHeight:100, border:'1px solid #e2e1e7', borderRadius:8, padding:12, marginTop:8, fontSize:14, resize:'vertical'}} />
        <div style={{fontSize:12, color:'#999', textAlign:'right'}}>{charCount(message)} ký tự</div>
        <div>
          <div style={{display:'flex', justifyContent:'space-between', alignItems:'center', marginTop:4}}>
            <label style={{fontSize:13, fontWeight:700}}>Ảnh đính kèm (tùy chọn, 1 ảnh đầu)</label>
            <button className="outline" onClick={()=>setShowPicker(v=>!v)} disabled={!desktop || busy} style={{height:30, fontSize:13}}><Images size={12}/> {showPicker ? 'Ẩn' : 'Chọn ảnh'}</button>
          </div>
          {!desktop && showPicker && <div style={{fontSize:12, color:'#777', marginTop:4}}>Chọn ảnh chỉ hoạt động trong bản Tauri Desktop</div>}
          {showPicker && <div style={{marginTop:8, maxHeight:160, overflowY:'auto', border:'1px solid #eee', borderRadius:8, padding:8, background:'#fafafa'}}>
            {mediaItems.length ? <div style={{display:'grid', gridTemplateColumns:'repeat(3,1fr)', gap:6}}>
              {mediaItems.map(m => <div key={m.id} onClick={()=>setSelectedMediaId(prev=> prev===m.id ? null : m.id)} style={{border: selectedMediaId===m.id ? '2px solid #6558d6' : '1px solid #e8e8ed', borderRadius:8, overflow:'hidden', cursor:'pointer', background:'#fff'}}>
                <div style={{aspectRatio:1, overflow:'hidden', background:'#f5f5f5', display:'grid', placeItems:'center'}}>{m.mediaType==='image' ? <img src={m.url} alt={m.name} style={{width:'100%', height:'100%', objectFit:'cover'}}/> : <><video src={m.url} style={{width:'100%', height:'100%', objectFit:'cover'}}/><div style={{fontSize:11, color:'#777'}}><Film size={10}/> VIDEO</div></>}</div>
                <div style={{padding:4, fontSize:11, textAlign:'center', whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis', background: selectedMediaId===m.id ? '#f0eefc' : '#fff', color: selectedMediaId===m.id ? '#6558d6' : '#333'}}>{selectedMediaId===m.id ? '✓ ' : ''}{m.name}</div>
              </div>)}
            </div> : <span style={{fontSize:13, color:'#777'}}>Thư viện trống — hãy nhập media trước</span>}
          </div>}
          {selectedMediaId && <div style={{fontSize:12, color:'#6558d6', marginTop:4}}>Đã chọn 1 ảnh để đăng kèm (ảnh đầu)</div>}
        </div>
        {err && <div className="error-box" style={{marginTop:8}}>{err}</div>}
        {msg && <div className="desktop-notice" style={{background:'#eef7ee', borderColor:'#cde9cd', color:'#2e6b2e', marginTop:8, padding:8}}><CircleCheck size={14}/><span>{msg}</span></div>}
        <div style={{display:'flex', justifyContent:'flex-end', gap:8}}>
          <button className="outline" onClick={onClose} disabled={busy} style={{height:36}}><X size={14}/> Đóng</button>
          <button className="primary" onClick={handlePublish} disabled={busy || !pageId || !message.trim()} style={{height:36}}><Send size={14}/> Đăng ngay</button>
        </div>
      </div>
    </div>
  )
}

const CONTENT_STATUS_LABELS = { draft: 'Nháp', approved: 'Đã duyệt', archived: 'Lưu trữ' }
const contentStatusLabel = (s) => CONTENT_STATUS_LABELS[s] || s

const resolveStyleLabel = (styles, value) => {
  const raw = value == null ? '' : String(value)
  const match = (styles || []).find(s => s.id === raw)
  return match ? match.name : raw
}

const filterLabelKey = (value) => String(value == null ? '' : value).trim().toLocaleLowerCase('vi')

const distinctSortedLabels = (values) => {
  const seen = new Set()
  const labels = []
  for (const value of values) {
    const label = String(value == null ? '' : value).trim()
    if (!label) continue
    const key = filterLabelKey(label)
    if (seen.has(key)) continue
    seen.add(key)
    labels.push(label)
  }
  return labels.sort((a, b) => a.localeCompare(b, 'vi'))
}

const CONTENT_PREVIEW_LIMITS = { body: 200, topic: 80, prompt: 120 }

const previewText = (value, maxChars) => {
  const text = String(value == null ? '' : value).replace(/\s+/g, ' ').trim()
  const chars = [...text]
  if (chars.length <= maxChars) return text
  return `${chars.slice(0, maxChars).join('').trimEnd()}…`
}

function SuggestInput({ ariaLabel, placeholder, value, onChange, options }) {
  const [open, setOpen] = useState(false)
  const [highlighted, setHighlighted] = useState(-1)
  const wrapRef = useRef(null)
  const listRef = useRef(null)
  const listId = useId()

  const needle = filterLabelKey(value)
  const filtered = (needle
    ? options.filter(option => filterLabelKey(option).includes(needle))
    : options
  ).slice(0, 50)
  const showMenu = open && options.length > 0
  const activeIndex = showMenu && highlighted >= 0 && highlighted < filtered.length
    ? highlighted
    : -1
  const optionId = (index) => `${listId}-option-${index}`

  const closeMenu = () => {
    setOpen(false)
    setHighlighted(-1)
  }

  useEffect(() => {
    if (!open) return
    const closeFromOutside = () => {
      setOpen(false)
      setHighlighted(-1)
    }
    const handleMouseDown = (event) => {
      if (!wrapRef.current?.contains(event.target)) closeFromOutside()
    }
    document.addEventListener('mousedown', handleMouseDown)
    window.addEventListener('blur', closeFromOutside)
    return () => {
      document.removeEventListener('mousedown', handleMouseDown)
      window.removeEventListener('blur', closeFromOutside)
    }
  }, [open])

  useEffect(() => {
    if (!showMenu || activeIndex < 0) return
    listRef.current
      ?.querySelector(`[data-option-index="${activeIndex}"]`)
      ?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }, [showMenu, activeIndex])

  const selectOption = (option) => {
    onChange(option)
    closeMenu()
  }

  const handleKeyDown = (event) => {
    if (event.nativeEvent.isComposing) return
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      if (!showMenu) {
        setOpen(true)
        setHighlighted(filtered.length ? 0 : -1)
        return
      }
      if (!filtered.length) return
      setHighlighted(prev => (prev < 0 || prev >= filtered.length - 1 ? 0 : prev + 1))
    } else if (event.key === 'ArrowUp') {
      event.preventDefault()
      if (!showMenu) {
        setOpen(true)
        setHighlighted(filtered.length ? filtered.length - 1 : -1)
        return
      }
      if (!filtered.length) return
      setHighlighted(prev => (prev <= 0 ? filtered.length - 1 : prev - 1))
    } else if (event.key === 'Enter') {
      if (showMenu && activeIndex >= 0) {
        event.preventDefault()
        selectOption(filtered[activeIndex])
      }
    } else if (event.key === 'Escape') {
      if (open) {
        event.preventDefault()
        closeMenu()
      }
    } else if (event.key === 'Tab') {
      closeMenu()
    }
  }

  return <div ref={wrapRef} style={{position:'relative', flex:'1 1 0', minWidth:0, display:'flex'}}>
    <input
      role="combobox"
      aria-label={ariaLabel}
      aria-autocomplete="list"
      aria-expanded={showMenu}
      aria-controls={listId}
      aria-activedescendant={activeIndex >= 0 ? optionId(activeIndex) : undefined}
      aria-haspopup="listbox"
      autoComplete="off"
      spellCheck={false}
      placeholder={placeholder}
      value={value}
      onChange={event => { onChange(event.target.value); setOpen(true); setHighlighted(-1) }}
      onFocus={() => setOpen(true)}
      onClick={() => setOpen(true)}
      onKeyDown={handleKeyDown}
      onBlur={event => {
        if (event.relatedTarget && !wrapRef.current?.contains(event.relatedTarget)) closeMenu()
      }}
      style={{width:'100%', minWidth:0, height:32, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 10px', fontSize:13, background:'#fff', color:'#24232d'}}
    />
    <div
      id={listId}
      ref={listRef}
      role="listbox"
      aria-label={`${ariaLabel} gợi ý`}
      style={{position:'absolute', top:'calc(100% + 4px)', left:0, right:0, zIndex:20, display:showMenu ? 'block' : 'none', background:'#fff', border:'1px solid #e9e9ee', borderRadius:8, boxShadow:'0 8px 24px #0000001f', padding:4, maxHeight:240, overflowY:'auto', overflowX:'hidden', overscrollBehavior:'contain', scrollbarColor:'#c9c7d2 #fff', scrollbarWidth:'thin'}}
    >
      {showMenu && (filtered.length
        ? filtered.map((option, index) => <div
            key={option}
            id={optionId(index)}
            role="option"
            aria-selected={index === activeIndex}
            data-option-index={index}
            onMouseDown={event => event.preventDefault()}
            onMouseEnter={() => setHighlighted(index)}
            onClick={() => selectOption(option)}
            style={{padding:'7px 10px', borderRadius:7, fontSize:13, lineHeight:'18px', cursor:'pointer', whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis', userSelect:'none', color:index === activeIndex ? '#6558d6' : '#24232d', background:index === activeIndex ? '#f0eefc' : 'transparent', fontWeight:index === activeIndex ? 700 : 500}}
          >{option}</div>)
        : <div role="option" aria-disabled="true" onMouseDown={event => event.preventDefault()} style={{padding:'9px 10px', fontSize:12, color:'#8e8c96'}}>Không có gợi ý</div>)}
    </div>
  </div>
}

function ContentWarehousePage({ resolveStyleName }) {
  const desktop = isDesktop()
  const [items, setItems] = useState([])
  const [filter, setFilter] = useState('all')
  const [query, setQuery] = useState('')
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState('')
  const [scheduleFor, setScheduleFor] = useState(null)
  const [editing, setEditing] = useState(null)
  const [topicQuery, setTopicQuery] = useState('')
  const [styleQuery, setStyleQuery] = useState('')

  const refresh = async () => {
    if (!desktop) return
    setBusy(true)
    try {
      const list = await invokeDesktop('list_content')
      setItems(list)
      setErr('')
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { refresh() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const remove = async (id) => {
    if (!window.confirm('Xóa nội dung này?')) return
    try { await invokeDesktop('delete_content', { id }); await refresh() } catch (e) { setErr(String(e)) }
  }
  const approve = async (id) => {
    try { await invokeDesktop('update_content_status', { id, status: 'approved' }); await refresh() } catch (e) { setErr(String(e)) }
  }

  const topicOptions = distinctSortedLabels(items.map(it => it.topic))
  const styleOptions = distinctSortedLabels(items.map(it => resolveStyleName(it.style)))

  const queryNeedle = query.trim().toLowerCase()
  const topicNeedle = topicQuery.trim().toLowerCase()
  const styleNeedle = styleQuery.trim().toLowerCase()

  const filtered = items.filter(it => {
    if (filter !== 'all' && it.status !== filter) return false
    if (queryNeedle && !`${it.title} ${it.body} ${it.prompt}`.toLowerCase().includes(queryNeedle)) return false
    if (topicNeedle && !String(it.topic == null ? '' : it.topic).toLowerCase().includes(topicNeedle)) return false
    if (styleNeedle && !`${resolveStyleName(it.style)} ${it.customStyle || ''}`.toLowerCase().includes(styleNeedle)) return false
    return true
  })

  const formatDate = (iso) => {
    try { return new Intl.DateTimeFormat('vi-VN', { day:'2-digit', month:'2-digit', hour:'2-digit', minute:'2-digit'}).format(new Date(iso)) } catch { return iso }
  }

  return <section className="media-page">
    <div className="page-heading"><div><p>KHO NỘI DUNG</p><h1>Kho nội dung</h1><span>Lưu trữ nội dung đã tạo, gắn media, duyệt và chuẩn bị lịch đăng.</span></div><div className="media-actions"><button className="outline" onClick={refresh} disabled={!desktop || busy}><RefreshCw size={16}/> Làm mới</button></div></div>
    {!desktop && <div className="desktop-notice"><HardDrive size={23}/><div><strong>Hãy mở bằng ứng dụng FlowPost AI Desktop</strong><span>Chỉ bản Tauri mới đọc được kho cục bộ.</span></div></div>}
    {err && <div className="error-box">{err}</div>}
    <div className="media-summary" style={{flexWrap:'wrap'}}>
      <div><FileText size={18}/><span><b>{items.length}</b> tổng</span></div>
      <div><CircleCheck size={18}/><span><b>{items.filter(i=>i.status==='draft').length}</b> nháp</span></div>
      <div><Zap size={18}/><span><b>{items.filter(i=>i.status==='approved').length}</b> đã duyệt</span></div>
      <div style={{flex:1, minWidth:0, display:'flex', gap:8, padding:0, border:'none', background:'transparent'}}>
        <input spellCheck={false} aria-label="Tìm theo tiêu đề hoặc nội dung" placeholder="Tiêu đề, nội dung" value={query} onChange={e=>setQuery(e.target.value)} style={{flex:'1 1 0', minWidth:0, height:32, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 10px', fontSize:13}}/>
        <SuggestInput ariaLabel="Tìm theo chủ đề" placeholder="Chủ đề" value={topicQuery} onChange={setTopicQuery} options={topicOptions}/>
        <SuggestInput ariaLabel="Tìm theo style" placeholder="Style" value={styleQuery} onChange={setStyleQuery} options={styleOptions}/>
      </div>
    </div>
    <div style={{display:'flex', gap:8, marginBottom:14}}>
      {[['all','Tất cả'],['draft','Draft'],['approved','Approved'],['archived','Archived']].map(([value,label]) => <button key={value} onClick={()=>setFilter(value)} className={filter===value ? 'primary' : 'outline'} style={{height:32, fontSize:14}}>{label}</button>)}
    </div>
    {filtered.length ? <div className="panel" style={{padding:0}}>{filtered.map((it, index) => <article key={it.id} style={{padding:'12px 16px', display:'flex', gap:16, alignItems:'flex-start', borderBottom: index === filtered.length - 1 ? 'none' : '1px solid #f0f0f4'}}>
      <div style={{flex:1, minWidth:0, display:'flex', flexDirection:'column', gap:6}}>
        <div style={{display:'flex', justifyContent:'space-between', alignItems:'center', gap:8}}>
          <span style={{fontSize:12, fontWeight:800, color: it.style==='Custom' ? '#d87642' : '#6558d6', background: it.style==='Custom' ? '#fff0e6' : '#f0eefc', padding:'3px 7px', borderRadius:10}}>{resolveStyleName(it.style)}{it.customStyle ? `:${it.customStyle}` : ''}</span>
          <span style={{fontSize:12, color: it.status==='approved' ? '#2e7d32' : '#777', background: it.status==='approved' ? '#e8f5e9' : '#f5f5f5', padding:'3px 7px', borderRadius:10}}>{contentStatusLabel(it.status)}</span>
        </div>
        <strong title={it.title?.trim() ? it.title : undefined} style={{fontSize:14, lineHeight:1.4, display:'-webkit-box', WebkitLineClamp:2, WebkitBoxOrient:'vertical', overflow:'hidden', whiteSpace:'pre-line', wordBreak:'break-word'}}>{it.title?.trim() ? it.title : 'Chưa có tiêu đề'}</strong>
        <span style={{fontSize:13, color:'#555', display:'-webkit-box', WebkitLineClamp:2, WebkitBoxOrient:'vertical', overflow:'hidden', whiteSpace:'pre-line', wordBreak:'break-word'}}>{previewText(it.body, CONTENT_PREVIEW_LIMITS.body)}</span>
        <div style={{display:'flex', alignItems:'center', minWidth:0, fontSize:12, color:'#888', whiteSpace:'nowrap'}}>
          <span style={{flex:1, minWidth:0, overflow:'hidden', textOverflow:'ellipsis', whiteSpace:'nowrap'}}>{it.topic ? `Chủ đề: ${previewText(it.topic, CONTENT_PREVIEW_LIMITS.topic)} • ` : ''}Prompt: {previewText(it.prompt, CONTENT_PREVIEW_LIMITS.prompt)}</span>
          <span style={{flexShrink:0}}> • {formatDate(it.createdAt)} • {it.mediaIds?.length || 0} media</span>
        </div>
      </div>
      <div style={{display:'flex', gap:8, flexWrap:'wrap', flexShrink:0}}>
        <button className="outline" onClick={()=>approve(it.id)} disabled={it.status==='approved' || busy} style={{height:30, fontSize:13}}><CircleCheck size={12}/> Duyệt</button>
        <button className="primary" onClick={()=>setScheduleFor(it.id)} disabled={it.status!=='approved' || busy} title={it.status!=='approved' ? 'Hãy duyệt trước' : ''} style={{height:30, fontSize:13}}><CalendarDays size={12}/> Lên lịch</button>
        <button className="outline" onClick={()=>remove(it.id)} disabled={busy} style={{height:30, fontSize:13}}><Trash2 size={12}/> Xóa</button>
        <button className="outline" onClick={()=>setEditing({...it})} disabled={busy} style={{height:30, fontSize:13}}><Pencil size={12}/> Sửa</button>
      </div>
    </article>)}</div> : <div className="media-empty"><div><FileText size={34}/></div><h2>{items.length===0 ? 'Kho nội dung đang trống' : 'Không có kết quả'}</h2><p>{items.length===0 ? 'Hãy tạo nội dung ở AI Content và Lưu vào kho.' : 'Thử đổi bộ lọc hoặc từ khóa.'}</p></div>}
    {scheduleFor && <ScheduleModal contentId={scheduleFor} onClose={()=>setScheduleFor(null)} onCreated={refresh} />}
    {editing && <div className="modal-overlay" onMouseDown={e=>{ if (e.target === e.currentTarget) setEditing(null) }} style={{position:'fixed', inset:0, background:'#0008', display:'grid', placeItems:'center', zIndex:50}}>
      <div className="modal" onClick={e=>e.stopPropagation()} style={{maxWidth:600, width:'90%', background:'#fff', borderRadius:12, padding:20, boxShadow:'0 8px 30px rgba(0,0,0,.2)'}}>
        <h2 style={{margin:0, fontSize:17}}>Sửa nội dung</h2>
        <div style={{display:'flex', flexDirection:'column', gap:10, marginTop:14}}>
          <input spellCheck={false} value={editing.title} onChange={e=>setEditing({...editing, title:e.target.value})} placeholder="Tiêu đề" style={{padding:'8px 10px', border:'1px solid #e2e1e7', borderRadius:8, fontSize:14}}/>
          <textarea spellCheck={false} value={editing.body} onChange={e=>setEditing({...editing, body:e.target.value})} placeholder="Nội dung" rows={5} style={{padding:'8px 10px', border:'1px solid #e2e1e7', borderRadius:8, fontSize:14, resize:'vertical'}}/>
          <div style={{fontSize:12, color:'#999', textAlign:'right'}}>{charCount(editing.body)} ký tự</div>
          <input spellCheck={false} value={editing.prompt} onChange={e=>setEditing({...editing, prompt:e.target.value})} placeholder="Prompt" style={{padding:'8px 10px', border:'1px solid #e2e1e7', borderRadius:8, fontSize:14}}/>
          <div style={{display:'flex', gap:8, justifyContent:'flex-end', marginTop:10}}>
            <button className="outline" onClick={()=>setEditing(null)} style={{height:34}}>Hủy</button>
            <button className="primary" onClick={async () => {
              if (!editing.title.trim() && !editing.body.trim()) { setErr('Tiêu đề hoặc nội dung không được để trống'); return }
              if (!editing.prompt.trim()) { setErr('Prompt không được để trống'); return }
              setBusy(true); setErr('')
              try {
                await invokeDesktop('update_content', {
                  payload: {
                    contentId: editing.id, title: editing.title, body: editing.body,
                    prompt: editing.prompt, style: resolveStyleName(editing.style), customStyle: editing.customStyle || null,
                    mediaIds: editing.mediaIds || [], topic: editing.topic || null
                  }
                })
                setEditing(null); await refresh()
              } catch (e) { setErr(String(e)) } finally { setBusy(false) }
            }} style={{height:34}}>Lưu</button>
          </div>
        </div>
      </div>
    </div>}
  </section>
}

function SchedulePage() {
  const desktop = isDesktop()
  const [schedules, setSchedules] = useState([])
  const [contents, setContents] = useState([])
  const [err, setErr] = useState('')
  const [busy, setBusy] = useState(false)
  const [month, setMonth] = useState(() => { const d=new Date(); return new Date(d.getFullYear(), d.getMonth(), 1) })
  const [tab, setTab] = useState('Calendar')
  const [platformFilter, setPlatformFilter] = useState('Tất cả')
  const [rescheduleFor, setRescheduleFor] = useState(null)
  const [rescheduleTime, setRescheduleTime] = useState('')
  const [dayDetail, setDayDetail] = useState(null)

  const refresh = async () => {
    if (!desktop) return
    setBusy(true)
    try {
      const [s, c] = await Promise.all([invokeDesktop('list_schedule'), invokeDesktop('list_content')])
      setSchedules(s); setContents(c); setErr('')
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { refresh() }, []) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (!desktop) return
    const id = setInterval(refresh, 4000)
    return () => clearInterval(id)
  }, [desktop]) // eslint-disable-line react-hooks/exhaustive-deps

  const deleteItem = async (id) => {
    if (!window.confirm('Xóa lịch này?')) return
    try { await invokeDesktop('delete_schedule', { id }); await refresh() } catch (e) { setErr(String(e)) }
  }

  const cancelItem = async (id) => {
    if (!window.confirm('Hủy lịch (giữ lại lịch sử)?')) return
    try { await invokeDesktop('update_schedule_status', { id, status: 'Cancelled' }); await refresh() } catch (e) { setErr(String(e)) }
  }

  const openReschedule = (item) => {
    setRescheduleFor(item)
    const next = new Date(new Date(item.scheduledAt).getTime() + 600000).toISOString().slice(0,16)
    setRescheduleTime(next); setErr('')
  }
  const handleReschedule = async () => {
    if (!rescheduleFor) return
    if (!rescheduleTime) { setErr('Vui lòng chọn thời gian'); return }
    setBusy(true); setErr('')
    try {
      const iso = new Date(rescheduleTime).toISOString()
      await invokeDesktop('reschedule', { id: rescheduleFor.id, scheduledAt: iso })
      setRescheduleFor(null); setRescheduleTime(''); await refresh()
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  const contentMap = new Map(contents.map(c => [c.id, c]))
  const filteredSchedules = platformFilter === 'Tất cả' ? schedules : schedules.filter(s => s.platform === platformFilter)

  const daysInMonth = () => {
    const y = month.getFullYear(), m = month.getMonth()
    const firstDay = new Date(y, m, 1).getDay() // 0 Sun
    const startOffset = (firstDay + 6) % 7 // Mon=0
    const days = new Date(y, m+1, 0).getDate()
    const cells = []
    for (let i=0;i<startOffset;i++) cells.push(null)
    for (let d=1; d<=days; d++) cells.push(new Date(y, m, d))
    return cells
  }
  const cells = daysInMonth()
  const isToday = (d) => { if(!d) return false; const t=new Date(); return d.getDate()===t.getDate() && d.getMonth()===t.getMonth() && d.getFullYear()===t.getFullYear() }
  const schedulesForDay = (d) => {
    if (!d) return []
    return filteredSchedules.filter(s => {
      const sd = new Date(s.scheduledAt)
      return sd.getDate()===d.getDate() && sd.getMonth()===d.getMonth() && sd.getFullYear()===d.getFullYear()
    })
  }

  const formatTime = (iso) => new Intl.DateTimeFormat('vi-VN', { hour:'2-digit', minute:'2-digit'}).format(new Date(iso))
  const formatFull = (iso) => new Intl.DateTimeFormat('vi-VN', { day:'2-digit', month:'2-digit', hour:'2-digit', minute:'2-digit'}).format(new Date(iso))

  return <section className="media-page">
    <div className="page-heading"><div><p>LỊCH ĐĂNG BÀI</p><h1>Lịch đăng bài</h1><span>Lịch đăng theo tháng và hàng đợi · Cảnh báo trùng &lt;30 phút nhưng cho phép đăng trùng nếu chấp nhận.</span></div><div className="media-actions"><button className="outline" onClick={refresh} disabled={!desktop || busy}><RefreshCw size={16}/> Làm mới</button></div></div>
    {!desktop && <div className="desktop-notice"><HardDrive size={23}/><div><strong>Hãy mở bằng ứng dụng FlowPost AI Desktop</strong><span>Chỉ bản Tauri mới đọc được lịch.</span></div></div>}
    {err && <div className="error-box">{err}</div>}
    <div style={{display:'flex', gap:8, marginBottom:14, flexWrap:'wrap', alignItems:'center'}}>
      <button className={tab==='Calendar'?'primary':'outline'} onClick={()=>setTab('Calendar')} style={{height:32}}><CalendarDays size={14}/> Lịch tháng</button>
      <button className={tab==='Queue'?'primary':'outline'} onClick={()=>setTab('Queue')} style={{height:32}}><Clock3 size={14}/> Hàng đợi ({filteredSchedules.length})</button>
      <select value={platformFilter} onChange={e=>setPlatformFilter(e.target.value)} style={{height:32, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', fontSize:14}}>
        <option>Tất cả</option><option>Sống Tích Cực</option><option>Daily Motivation</option><option>Chill Mỗi Ngày</option>
      </select>
    </div>
    {tab==='Calendar' ? <>
      <div className="panel" style={{padding:14, display:'flex', justifyContent:'space-between', alignItems:'center'}}>
        <button className="outline" onClick={()=>setMonth(new Date(month.getFullYear(), month.getMonth()-1, 1))} style={{height:32}}>‹ Tháng trước</button>
        <strong style={{fontSize:16}}>Tháng {month.getMonth()+1}/{month.getFullYear()}</strong>
        <button className="outline" onClick={()=>setMonth(new Date(month.getFullYear(), month.getMonth()+1, 1))} style={{height:32}}>Tháng sau ›</button>
      </div>
      <div className="panel" style={{padding:12, marginTop:12}}>
        <div style={{display:'grid', gridTemplateColumns:'repeat(7,1fr)', gap:8, fontSize:13, fontWeight:700, color:'#777', marginBottom:8}}>
          <span>T2</span><span>T3</span><span>T4</span><span>T5</span><span>T6</span><span>T7</span><span>CN</span>
        </div>
        <div style={{display:'grid', gridTemplateColumns:'repeat(7,1fr)', gap:8}}>
          {cells.map((d, idx) => {
            const list = schedulesForDay(d)
            const today = isToday(d)
            return <div key={idx} className="day-cell" onClick={()=> d && list.length && setDayDetail(d)} style={{minHeight:92, border: today ? '2px solid #6558d6' : '1px solid #e9e9ee', borderRadius:10, padding:8, background: d ? (today ? '#f7f6fe' : '#fff') : '#f9f9f9', cursor: d && list.length ? 'pointer' : 'default'}}>
               {d && <><div style={{fontSize:14, fontWeight: today ? 800 : 600, color: today ? '#6558d6' : '#333'}}>{d.getDate()}</div>
              <div style={{marginTop:6, display:'flex', flexDirection:'column', gap:4}}>
                {list.slice(0,3).map(s => {
                  const c = contentMap.get(s.contentId)
                  return <span key={s.id} style={{fontSize:11, background: s.platform==='Sống Tích Cực' ? '#efedff' : s.platform==='Daily Motivation' ? '#fff3e0' : '#e8f5e9', color:'#333', padding:'2px 6px', borderRadius:6, whiteSpace:'nowrap', overflow:'hidden', textOverflow:'ellipsis'}}>{formatTime(s.scheduledAt)} {c ? c.title.slice(0,18) : s.contentId.slice(0,6)}</span>
                })}
                {list.length>3 && <span style={{fontSize:11, color:'#777'}}>+{list.length-3} nữa</span>}
                {list.length===0 && <span style={{fontSize:11, color:'#bbb'}}>—</span>}
              </div></>}
            </div>
          })}
        </div>
      </div>
    </> : <div className="panel" style={{padding:0, overflow:'hidden'}}>
      <div style={{padding:'12px 14px', borderBottom:'1px solid #eee', display:'flex', justifyContent:'space-between'}}>
        <strong style={{fontSize:14}}>Hàng đợi ({filteredSchedules.length})</strong><span style={{fontSize:13, color:'#777'}}>Sắp xếp theo thời gian</span>
      </div>
      {filteredSchedules.length ? <div className="table-wrap"><table><thead><tr><th>THỜI GIAN</th><th>NỘI DUNG</th><th>TRANG</th><th>TRẠNG THÁI</th><th></th></tr></thead><tbody>{filteredSchedules.map(s => {
        const c = contentMap.get(s.contentId)
        const isScheduled = s.status === 'Scheduled'
        return <tr key={s.id}><td>{formatFull(s.scheduledAt)}</td><td><strong style={{fontSize:14}}>{c ? c.title : s.contentId}</strong><div style={{fontSize:12, color:'#777'}}>{c ? c.prompt.slice(0,40) : ''}</div></td><td><span style={{fontSize:13, background:'#f0eefc', padding:'3px 7px', borderRadius:10}}>{s.platform}</span></td><td><span style={{fontSize:12, padding:'3px 7px', borderRadius:10, background: s.status==='Scheduled' ? '#fff3cd' : s.status==='Published' ? '#e8f5e9' : s.status==='Failed' ? '#fdecea' : '#e8e8e8', color: s.status==='Scheduled' ? '#664d03' : s.status==='Published' ? '#2e7d32' : s.status==='Cancelled' ? '#555' : '#611a15'}}>{s.status}</span>{s.status === 'Failed' && s.lastError && <div style={{fontSize:11, color:'#c62828', marginTop:4, maxWidth:200, wordBreak:'break-word'}}>{s.lastError} (lần {s.retryCount || 0}/3)</div>}</td><td><div style={{display:'flex', gap:4}}><button className="outline" onClick={()=>openReschedule(s)} disabled={!isScheduled || busy} title={isScheduled ? '' : 'Chỉ lịch Scheduled mới dời được'} style={{height:30, fontSize:12}}><CalendarDays size={11}/> Dời</button><button className="outline" onClick={()=>cancelItem(s.id)} disabled={!isScheduled || busy} title={isScheduled ? '' : 'Chỉ lịch Scheduled mới hủy được'} style={{height:30, fontSize:12}}><X size={11}/> Hủy</button><button className="outline" onClick={()=>deleteItem(s.id)} style={{height:30, fontSize:13}}><Trash2 size={12}/> Xóa</button>{s.status === 'Failed' && <button className="outline" onClick={async () => { if (!window.confirm('Thử lại lịch này?')) return; try { await invokeDesktop('retry_schedule', { scheduleId: s.id }); await refresh() } catch (e) { setErr(String(e)) } }} disabled={busy} style={{height:30, fontSize:12}}><RefreshCw size={11}/> Thử lại</button>}</div></td></tr>
      })}</tbody></table></div> : <div className="media-empty" style={{padding:30}}><div><CalendarDays size={34}/></div><h2>Chưa có lịch nào</h2><p>Hãy duyệt nội dung ở Kho và bấm Lên lịch.</p></div>}
    </div>}
      {rescheduleFor && <div style={{position:'fixed', inset:0, background:'#0006', display:'grid', placeItems:'center', zIndex:55}} onClick={()=>setRescheduleFor(null)}>
        <div className="panel" style={{width:380, padding:16, display:'flex', flexDirection:'column', gap:10}} onClick={e=>e.stopPropagation()}>
          <h3 style={{margin:0, fontSize:15}}>Dời lịch: {rescheduleFor.platform}</h3>
          <input type="datetime-local" value={rescheduleTime} onChange={e=>setRescheduleTime(e.target.value)} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px'}}/>
          <div style={{fontSize:12, color:'#777'}}>Phải trong tương lai, tối thiểu 5 phút.</div>
          {err && <div className="error-box">{err}</div>}
          <div style={{display:'flex', justifyContent:'flex-end', gap:8}}>
            <button className="outline" onClick={()=>setRescheduleFor(null)} disabled={busy} style={{height:32}}>Đóng</button>
            <button className="primary" onClick={handleReschedule} disabled={busy} style={{height:32}}><CalendarDays size={14}/> Xác nhận</button>
          </div>
        </div>
      </div>}
      {dayDetail && <div style={{position:'fixed', inset:0, background:'#0006', display:'grid', placeItems:'center', zIndex:55}} onClick={()=>setDayDetail(null)}>
        <div className="panel" style={{width:360, padding:16, display:'flex', flexDirection:'column', gap:8}} onClick={e=>e.stopPropagation()}>
          <h3 style={{margin:0, fontSize:15}}>Lịch ngày {dayDetail.toLocaleDateString('vi-VN')}</h3>
          {schedulesForDay(dayDetail).map(s=> { const c=contentMap.get(s.contentId); return <div key={s.id} style={{display:'flex', justifyContent:'space-between', alignItems:'center', padding:'6px 8px', background:'#fafafa', borderRadius:6, fontSize:13}}><span>{formatTime(s.scheduledAt)} {c?c.title.slice(0,20):s.contentId.slice(0,6)} <span style={{color:'#777'}}>({s.status})</span></span><span style={{display:'flex', gap:4}}><button className="outline" onClick={()=>{ setDayDetail(null); openReschedule(s) }} disabled={s.status!=='Scheduled'} style={{height:28, fontSize:11}}>Dời</button><button className="outline" onClick={()=>{ cancelItem(s.id); setDayDetail(null) }} disabled={s.status!=='Scheduled'} style={{height:28, fontSize:11}}>Hủy</button></span></div> })}
          <div style={{display:'flex', justifyContent:'flex-end'}}><button className="outline" onClick={()=>setDayDetail(null)} style={{height:32}}>Đóng</button></div>
        </div>
      </div>}
  </section>
}

function DashboardSchedulePanel({ onViewAll, resolveStyleName }) {
  const desktop = isDesktop()
  const [todaySchedules, setTodaySchedules] = useState([])
  const [contentMap, setContentMap] = useState(new Map())
  useEffect(() => {
    if (!desktop) return
    const load = async () => {
      try {
        const [s, c] = await Promise.all([invokeDesktop('list_schedule'), invokeDesktop('list_content')])
        const todayStr = new Date().toDateString()
        const todayList = s.filter(x => new Date(x.scheduledAt).toDateString() === todayStr && x.status === 'Scheduled').sort((a,b)=> new Date(a.scheduledAt) - new Date(b.scheduledAt)).slice(0,3)
        setTodaySchedules(todayList)
        setContentMap(new Map(c.map(x=>[x.id, x])))
      } catch { /* ignore */ }
    }
    load()
    const id = setInterval(load, 4000)
    return () => clearInterval(id)
  }, [desktop])
  const formatTime = (iso) => new Intl.DateTimeFormat('vi-VN', { hour:'2-digit', minute:'2-digit'}).format(new Date(iso))
  const dotColor = (platform) => platform==='Sống Tích Cực' ? 'purple-dot' : platform==='Daily Motivation' ? 'orange-dot' : 'green-dot'
  return <div className="panel schedule-panel">
    <div className="panel-head"><div><h2>Lịch sắp tới</h2><p>{todaySchedules.length} tác vụ trong hôm nay</p></div><button onClick={onViewAll}><MoreHorizontal size={20}/></button></div>
    <div className="timeline">
      {todaySchedules.length ? todaySchedules.map(s => {
        const c = contentMap.get(s.contentId)
        return <div className="task" key={s.id}><time>{formatTime(s.scheduledAt)}</time><i className={`dot ${dotColor(s.platform)}`}/><div><strong>{c ? c.title.slice(0,28) : s.contentId.slice(0,8)}</strong><span>{s.platform} • {c ? resolveStyleName(c.style) : s.contentId}</span><small><Send size={12}/> Đã lên lịch</small></div></div>
      }) : <>
        <div className="task"><time>10:00</time><i className="dot purple-dot"/><div><strong>Chưa có lịch hôm nay</strong><span>Hãy tạo lịch từ Kho nội dung</span><small><CalendarDays size={12}/> Trống</small></div></div>
      </>}
    </div>
    <button className="schedule-link" onClick={onViewAll}>Xem tất cả lịch <span>→</span></button>
  </div>
}

function App() {
  const [active, setActive] = useState('Tổng quan')
  const [period, setPeriod] = useState('7 ngày qua')
  const [toast, setToast] = useState(false)
  const [contentCount, setContentCount] = useState(0)
  const [dashboardChartData, setDashboardChartData] = useState([])
  const [dashboardRecent, setDashboardRecent] = useState([])
  const [dashboardPublished, setDashboardPublished] = useState(0)
  const [dashboardScheduled, setDashboardScheduled] = useState(0)
  const [dashboardPostsThisMonth, setDashboardPostsThisMonth] = useState(0)
  const [dashboardTotalInteractions, setDashboardTotalInteractions] = useState('—')
  const [dashboardTotalSub, setDashboardTotalSub] = useState('chờ Graph insights')
  const [dashboardEngagementRate, setDashboardEngagementRate] = useState('—')
  const [styleList, setStyleList] = useState([])
  const [hoveredRecent, setHoveredRecent] = useState(null)
  const desktop = isDesktop()
  const today = new Intl.DateTimeFormat('vi-VN', { weekday: 'long', day: '2-digit', month: 'long' }).format(new Date())
  const createPost = () => { setActive('AI Content'); setToast(true); setTimeout(() => setToast(false), 2600) }
  const resolveStyleName = (value) => resolveStyleLabel(styleList, value)

  useEffect(() => {
    if (!desktop) return
    let cancelled = false
    invokeDesktop('list_styles')
      .then(list => { if (!cancelled) setStyleList(Array.isArray(list) ? list : []) })
      .catch(() => {})
    return () => { cancelled = true }
  }, [desktop])

  useEffect(() => {
    if (!desktop) return
    const load = async () => {
      try {
        const [contents, schedules] = await Promise.all([invokeDesktop('list_content'), invokeDesktop('list_schedule')])
        setContentCount(contents.length)
        // Recent posts: 3 mới nhất từ content (fallback khi chưa có insights)
        let recent = [...contents].sort((a,b)=> new Date(b.createdAt) - new Date(a.createdAt)).slice(0,3).map((c,i) => {
          const styleLabel = resolveStyleLabel(styleList, c.style)
          return {
            title: c.title, page: styleLabel, date: new Date(c.createdAt).toLocaleDateString('vi-VN', { day:'2-digit', month:'2-digit', hour:'2-digit', minute:'2-digit'}),
            type: c.mediaIds && c.mediaIds.length ? 'Ảnh + văn bản' : 'Văn bản', status: contentStatusLabel(c.status), color: ['#f0a86e','#7469d5','#4b9b7d'][i%3], initials: styleLabel.slice(0,2).toUpperCase(), reach: '—', likes: '—', comments: '—', pending: c.status !== 'approved', tooltip: '', tooltipData: { impressions:'—', unique:'—', engaged:'—', clicks:'—', reactions:'—', videoViews:'—' }
          }
        })
        setDashboardRecent(recent)
        setDashboardTotalInteractions('—'); setDashboardTotalSub('chờ Graph insights'); setDashboardEngagementRate('—')
        // Thử lấy toàn bộ 6 metrics thật từ Graph API (post_impressions, post_impressions_unique, post_engaged_users, post_clicks, post_reactions_by_type_total, post_video_views)
        try {
          const pages = await invokeDesktop('list_facebook_pages', { token: '' })
          if (pages.length) {
            const pageId = pages[0].id
            const postsRes = await invokeDesktop('list_page_posts', { pageId, limit: 5 })
            const fbPosts = postsRes?.data || (Array.isArray(postsRes) ? postsRes : [])
            if (Array.isArray(fbPosts) && fbPosts.length) {
              const getVal = (data, name) => {
                const m = (data || []).find(x => x.name === name)
                if (!m) return '—'
                const v = m.values?.[0]?.value
                if (v == null) return '—'
                if (typeof v === 'object') return JSON.stringify(v)
                return String(v)
              }
              const enriched = await Promise.all(fbPosts.slice(0,3).map(async (fp, idx) => {
                try {
                  const ins = await invokeDesktop('get_post_insights', { postId: fp.id })
                  const data = ins?.data || []
                  const msg = fp.message ? fp.message.slice(0,40) : fp.id
                  return {
                    title: msg, page: pages[0].name, date: new Date(fp.created_time).toLocaleDateString('vi-VN', { day:'2-digit', month:'2-digit', hour:'2-digit', minute:'2-digit'}),
                    type: fp.full_picture ? 'Ảnh + văn bản' : 'Văn bản', status: 'Đã đăng', color: ['#f0a86e','#7469d5','#4b9b7d'][idx%3], initials: pages[0].name.slice(0,2).toUpperCase(),
                    reach: getVal(data, 'post_impressions_unique'), likes: getVal(data, 'post_reactions_by_type_total'), comments: getVal(data, 'post_clicks'), pending: false,
                    tooltip: `Impressions: ${getVal(data,'post_impressions')} | Unique: ${getVal(data,'post_impressions_unique')} | Engaged: ${getVal(data,'post_engaged_users')} | Clicks: ${getVal(data,'post_clicks')} | Reactions: ${getVal(data,'post_reactions_by_type_total')} | VideoViews: ${getVal(data,'post_video_views')}`,
                    tooltipData: { impressions: getVal(data,'post_impressions'), unique: getVal(data,'post_impressions_unique'), engaged: getVal(data,'post_engaged_users'), clicks: getVal(data,'post_clicks'), reactions: getVal(data,'post_reactions_by_type_total'), videoViews: getVal(data,'post_video_views') }
                  }
                } catch { return null }
              }))
              const filtered = enriched.filter(Boolean)
              if (filtered.length) {
                recent = filtered
                setDashboardRecent(filtered)
                // Tính tổng tương tác và tỷ lệ từ insights
                let total = 0
                let totalUnique = 0
                for (const r of filtered) {
                  const d = r.tooltipData
                  if (!d) continue
                  const engaged = Number(String(d.engaged).replace(/[^0-9.-]/g,'')) || 0
                  const clicks = Number(String(d.clicks).replace(/[^0-9.-]/g,'')) || 0
                  let reactionsNum = 0
                  try { const obj = JSON.parse(d.reactions); if (typeof obj === 'object' && obj !== null) reactionsNum = Object.values(obj).reduce((s,v)=> s + (Number(v)||0),0); else reactionsNum = Number(d.reactions)||0 } catch { reactionsNum = Number(String(d.reactions).replace(/[^0-9.-]/g,''))||0 }
                  total += engaged + clicks + reactionsNum
                  const unique = Number(String(d.unique).replace(/[^0-9.-]/g,'')) || 0
                  totalUnique += unique
                }
                if (total) { setDashboardTotalInteractions(String(total)); setDashboardTotalSub(`${filtered.length} bài có insights`) } else { setDashboardTotalInteractions('—'); setDashboardTotalSub('chờ Graph insights') }
                if (total && totalUnique) { setDashboardEngagementRate(`${((total/totalUnique)*100).toFixed(1)}%`) } else { setDashboardEngagementRate('—') }
              } else {
                setDashboardTotalInteractions('—'); setDashboardTotalSub('chờ Graph insights'); setDashboardEngagementRate('—')
              }
            }
          }
        } catch { /* không có token read_insights thì giữ '—' */ }
        // Stats
        const now = new Date()
        const thisMonthContents = contents.filter(c => { const d=new Date(c.createdAt); return d.getMonth()===now.getMonth() && d.getFullYear()===now.getFullYear() }).length
        const published = schedules.filter(s=>s.status==='Published').length
        const scheduled = schedules.filter(s=>s.status==='Scheduled').length
        setDashboardPostsThisMonth(thisMonthContents)
        setDashboardPublished(published)
        setDashboardScheduled(scheduled)
        // Chart 7 ngày gần nhất: đếm schedule Published+Scheduled theo ngày
        const days = []
        for (let i=6; i>=0; i--) {
          const d = new Date(); d.setDate(now.getDate()-i)
          const label = weekdayLabel(d)
          const count = schedules.filter(s => { const sd=new Date(s.scheduledAt); return sd.toDateString()===d.toDateString() && (s.status==='Published' || s.status==='Scheduled') }).length
          days.push({ day: label, posts: count, engagement: count })
        }
        setDashboardChartData(days)
      } catch { /* ignore - keep previous */ }
    }
    load()
    const id = setInterval(load, 4000)
    return () => clearInterval(id)
  }, [desktop, active, styleList])

  return <div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><div className="brand-mark"><WandSparkles size={21}/></div><span>FlowPost <b>AI</b></span></div>
      <nav>{nav.map(([name, Icon]) => <button className={active === name ? 'active' : ''} onClick={() => setActive(name)} key={name}><Icon size={19}/><span>{name}</span>{name === 'Kho nội dung' && <em>{contentCount}</em>}</button>)}</nav>
      <div className="sidebar-bottom">
        <button onClick={() => setActive('Cài đặt')} className={active === 'Cài đặt' ? 'active' : ''}><Settings size={19}/><span>Cài đặt</span></button>
        <SettingsStatus/>
        <div className="profile"><div className="avatar">NT</div><div><strong>Nguyễn Xuân Thoán</strong><span>Tác giả</span></div></div>
      </div>
    </aside>

    <main>
      <header><div className="search"><Search size={18}/><input spellCheck={false} aria-label="Tìm kiếm" placeholder="Tìm kiếm nội dung, bài viết..." disabled style={{opacity:0.6}}/></div><div className="head-actions"><button className="primary" onClick={createPost}><Plus size={19}/> Tạo nội dung mới</button></div></header>
      <div className="content">
        {active === 'AI Content' ? <AiContentPage/> : active === 'Kho nội dung' ? <ContentWarehousePage resolveStyleName={resolveStyleName}/> : active === 'Thư viện media' ? <MediaLibrary/> : active === 'Lịch đăng bài' ? <SchedulePage/> : active === 'Lịch AI content' ? <AutoSchedulePage desktop={desktop} /> : active === 'Cài đặt' ? <SettingsPage/> : <>
        <section className="welcome"><div><p>{today}</p><h1>{getGreeting()}</h1><div className="welcome-sub">Hôm nay bạn có <b>{dashboardScheduled} bài viết</b> đang chờ được đăng.</div></div><button className="outline" onClick={()=>setActive('Lịch đăng bài')}><CalendarDays size={17}/> Xem lịch đăng bài</button></section>

        <section className="stats-grid">
          <StatCard icon={FileText} iconClass="purple" label="Bài viết tháng này" value={String(dashboardPostsThisMonth)} delta="" sub="từ Kho nội dung"/>
          <StatCard icon={Send} iconClass="green" label="Đã đăng thành công" value={String(dashboardPublished)} delta="" sub={`${dashboardScheduled} bài đang chờ`}/>
          <StatCard icon={Heart} iconClass="orange" label="Tổng tương tác" value={String(dashboardTotalInteractions)} delta="" sub={dashboardTotalSub}/>
          <StatCard icon={TrendingUp} iconClass="blue" label="Tỷ lệ tương tác" value={String(dashboardEngagementRate)} delta="" sub={dashboardEngagementRate==='—' ? 'chờ Graph insights' : 'trên tổng reach unique'}/>
        </section>

        <section className="middle-grid">
          <div className="panel chart-panel">
            <div className="panel-head"><div><h2>Hiệu suất nội dung</h2><p>Lượt đăng trong 7 ngày gần nhất</p></div><select value={period} onChange={e => setPeriod(e.target.value)}><option>7 ngày qua</option><option>30 ngày qua</option></select></div>
            <div className="chart-total"><strong>{dashboardChartData.reduce((s,c)=>s+c.engagement,0)}</strong><span> bài</span></div>
            <div className="chart-wrap"><ResponsiveContainer width="100%" height="100%"><AreaChart data={dashboardChartData.length ? dashboardChartData : [{day:'T2',posts:0,engagement:0},{day:'T3',posts:0,engagement:0},{day:'T4',posts:0,engagement:0},{day:'T5',posts:0,engagement:0},{day:'T6',posts:0,engagement:0},{day:'T7',posts:0,engagement:0},{day:'CN',posts:0,engagement:0}]} margin={{top: 8, right: 8, left: -20, bottom: 0}}><defs><linearGradient id="colorEng" x1="0" y1="0" x2="0" y2="1"><stop offset="5%" stopColor="#6558d6" stopOpacity={0.25}/><stop offset="95%" stopColor="#6558d6" stopOpacity={0}/></linearGradient></defs><CartesianGrid strokeDasharray="3 3" vertical={false} stroke="#ececf2"/><XAxis dataKey="day" axisLine={false} tickLine={false} tick={{fill:'#8c8b98', fontSize:14}}/><YAxis axisLine={false} tickLine={false} tick={{fill:'#aaa9b3', fontSize:13}}/><Tooltip contentStyle={{border:'none', borderRadius:10, boxShadow:'0 8px 30px #2222'}}/><Area type="monotone" dataKey="engagement" stroke="#6558d6" strokeWidth={2.5} fill="url(#colorEng)"/></AreaChart></ResponsiveContainer></div>
          </div>
          <DashboardSchedulePanel onViewAll={()=>setActive('Lịch đăng bài')} resolveStyleName={resolveStyleName} />
        </section>

        <section className="panel recent">
          <div className="panel-head"><div><h2>Bài viết gần đây</h2><p>Theo dõi trạng thái và hiệu quả bài đăng</p></div><button className="text-link">Xem tất cả <span>→</span></button></div>
          <div className="table-wrap"><table><thead><tr><th>NỘI DUNG</th><th>TRANG</th><th>THỜI GIAN</th><th>LOẠI</th><th>TRẠNG THÁI</th><th>TIẾP CẬN</th><th>TƯƠNG TÁC</th><th></th></tr></thead><tbody>{dashboardRecent.length ? dashboardRecent.map((p, i) => <tr key={p.title + i} onMouseEnter={()=>setHoveredRecent(i)} onMouseLeave={()=>setHoveredRecent(null)} style={{position:'relative'}}><td><div className="post-title"><div className={`thumb thumb${i+1}`}>{i === 0 ? '☀️' : i === 1 ? '“' : '▶'}</div><strong>{p.title}</strong></div></td><td><div className="page"><i style={{background:p.color}}>{p.initials}</i>{p.page}</div></td><td>{p.date}</td><td><span className="type">{p.type}</span></td><td><span className={p.pending ? 'status pending' : 'status success'}>{p.pending ? <Clock3 size={13}/> : <CircleCheck size={13}/>} {p.pending ? 'Đang xử lý' : p.status}</span></td><td style={{position:'relative'}}><strong>{p.reach}</strong>{hoveredRecent===i && p.tooltipData && p.tooltipData.impressions!=='—' && <div style={{position:'absolute', top:'100%', left:0, background:'#fff', border:'1px solid #e9e9ee', borderRadius:8, boxShadow:'0 8px 24px #2222', padding:10, zIndex:10, minWidth:240}}><div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap:'6px 12px', fontSize:13}}><span style={{color:'#777'}}>Impressions</span><b>{p.tooltipData.impressions}</b><span style={{color:'#777'}}>Unique</span><b>{p.tooltipData.unique}</b><span style={{color:'#777'}}>Engaged</span><b>{p.tooltipData.engaged}</b><span style={{color:'#777'}}>Clicks</span><b>{p.tooltipData.clicks}</b><span style={{color:'#777'}}>Reactions</span><b style={{wordBreak:'break-all'}}>{p.tooltipData.reactions}</b><span style={{color:'#777'}}>VideoViews</span><b>{p.tooltipData.videoViews}</b></div></div>}</td><td style={{position:'relative'}}><div className="engage"><span><Heart size={14}/> {p.likes || '—'}</span><span><MessageCircle size={14}/> {p.comments || '—'}</span></div>{hoveredRecent===i && p.tooltipData && p.tooltipData.impressions!=='—' && <div style={{position:'absolute', top:'100%', right:0, background:'#fff', border:'1px solid #e9e9ee', borderRadius:8, boxShadow:'0 8px 24px #2222', padding:10, zIndex:10, minWidth:240}}><div style={{display:'grid', gridTemplateColumns:'1fr 1fr', gap:'6px 12px', fontSize:13}}><span style={{color:'#777'}}>Impressions</span><b>{p.tooltipData.impressions}</b><span style={{color:'#777'}}>Unique</span><b>{p.tooltipData.unique}</b><span style={{color:'#777'}}>Engaged</span><b>{p.tooltipData.engaged}</b><span style={{color:'#777'}}>Clicks</span><b>{p.tooltipData.clicks}</b><span style={{color:'#777'}}>Reactions</span><b style={{wordBreak:'break-all'}}>{p.tooltipData.reactions}</b><span style={{color:'#777'}}>VideoViews</span><b>{p.tooltipData.videoViews}</b></div></div>}</td><td><button className="row-more" aria-label="Thao tác"><MoreHorizontal size={18}/></button></td></tr>) : <tr><td colSpan={8} style={{textAlign:'center', padding:20, color:'#777', fontSize:14}}>Chưa có bài viết — hãy tạo nội dung ở AI Content</td></tr>}</tbody></table></div>
        </section>
        </>}
        <footer><span>© 2026 FlowPost AI</span><span>Trạng thái hệ thống <i/> Hoạt động ổn định</span></footer>
      </div>
    </main>
    {toast && <div className="toast"><CircleCheck size={19}/><div><strong>Đã mở trình tạo nội dung</strong><span>Sẵn sàng sáng tạo bài viết mới cùng AI.</span></div></div>}
  </div>
}

function AutoSchedulePage({ desktop }) {
  const [items, setItems] = useState([])
  const [busy, setBusy] = useState(false)
  const [err, setErr] = useState('')
  const [msg, setMsg] = useState('')
  const [editing, setEditing] = useState(null)
  const [showModal, setShowModal] = useState(false)
  const [styles, setStyles] = useState([])
  const [providers, setProviders] = useState([])
  const [config, setConfig] = useState({ intervalSeconds: 60, enabled: true })
  const [form, setForm] = useState({
    name: '', recurrenceType: 'fixed', fixedTimes: '08:00', intervalMinutes: 10,
    prompt: '', style: '', providerId: '', model: '', autoApprove: false, enabled: true,
  })

  const load = async () => {
    if (!desktop) return
    setBusy(true)
    try {
      const [list, cfg, st, pv] = await Promise.all([
        invokeDesktop('list_auto_schedule'),
        invokeDesktop('get_auto_scheduler_config'),
        invokeDesktop('list_styles'),
        invokeDesktop('list_providers'),
      ])
      setItems(list || [])
      setConfig({ intervalSeconds: cfg.intervalSeconds, enabled: cfg.enabled })
      setStyles(st || [])
      setProviders(pv || [])
      if (!form.style && st && st.length) setForm(f => ({ ...f, style: st[0].name || st[0].id }))
      setErr('')
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }
  // eslint-disable-next-line react-hooks/set-state-in-effect
  useEffect(() => { load() }, []) // eslint-disable-line react-hooks/exhaustive-deps

  const recurrenceDisplay = (r) => {
    if (!r) return ''
    if (r.type === 'fixedTimes' || r.fixedTimes) {
      const times = r.times || r.fixedTimes || []
      return times.join(', ')
    }
    if (r.type === 'interval' || r.minutes != null) {
      const m = r.minutes || r.intervalMinutes
      return `Mỗi ${m} phút`
    }
    // handle camelCase from backend: recurrence has kind? Actually backend enum serialized as {"fixedTimes":{"times":[...]}} or {"interval":{"minutes":...}}
    if (r.fixedTimes) return (r.fixedTimes.times || []).join(', ')
    if (r.interval) return `Mỗi ${r.interval.minutes} phút`
    return JSON.stringify(r)
  }

  const normalizeRecurrence = (r) => {
    // backend serde enum: { "fixedTimes": { "times": [...] } } or { "interval": { "minutes": ... } }
    if (r.fixedTimes) return { kind: 'fixed', times: r.fixedTimes.times || [] }
    if (r.interval) return { kind: 'interval', minutes: r.interval.minutes }
    if (r.type === 'fixedTimes') return { kind: 'fixed', times: r.times || [] }
    if (r.type === 'interval') return { kind: 'interval', minutes: r.minutes }
    return { kind: 'fixed', times: [] }
  }

  const openCreate = () => {
    setEditing(null)
    setForm({ name: '', recurrenceType: 'fixed', fixedTimes: '08:00', intervalMinutes: 10, prompt: '', style: styles.length ? (styles[0].name || styles[0].id) : '', providerId: '', model: '', autoApprove: false, enabled: true })
    setShowModal(true); setErr(''); setMsg('')
  }
  const openEdit = (item) => {
    const rec = normalizeRecurrence(item.recurrence)
    setEditing(item)
    setForm({
      name: item.name,
      recurrenceType: rec.kind,
      fixedTimes: rec.kind === 'fixed' ? rec.times.join(', ') : '08:00',
      intervalMinutes: rec.kind === 'interval' ? rec.minutes : 10,
      prompt: item.prompt,
      style: resolveStyleLabel(styles, item.style),
      providerId: item.providerId || '',
      model: item.model || '',
      autoApprove: !!item.autoApprove,
      enabled: !!item.enabled,
    })
    setShowModal(true); setErr(''); setMsg('')
  }

  const buildRecurrence = () => {
    if (form.recurrenceType === 'fixed') {
      const times = form.fixedTimes.split(',').map(s => s.trim()).filter(Boolean)
      return { fixedTimes: { times } }
    }
    return { interval: { minutes: Number(form.intervalMinutes) } }
  }

  const save = async () => {
    setBusy(true); setErr(''); setMsg('')
    try {
      const recurrence = buildRecurrence()
      if (editing) {
        await invokeDesktop('update_auto_schedule', { id: editing.id, payload: {
          name: form.name, recurrence, prompt: form.prompt, style: form.style,
          providerId: form.providerId || null, model: form.model || null,
          autoApprove: form.autoApprove, enabled: form.enabled,
        } })
        setMsg('Đã cập nhật lịch AI')
      } else {
        await invokeDesktop('create_auto_schedule', { payload: {
          name: form.name, recurrence, prompt: form.prompt, style: form.style,
          providerId: form.providerId || null, model: form.model || null,
          autoApprove: form.autoApprove,
        } })
        setMsg('Đã tạo lịch AI')
      }
      setShowModal(false); await load()
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  const remove = async (id) => {
    if (!window.confirm('Xóa lịch AI này?')) return
    try { await invokeDesktop('delete_auto_schedule', { id }); await load() } catch (e) { setErr(String(e)) }
  }
  const toggle = async (item) => {
    try { await invokeDesktop('toggle_auto_schedule', { id: item.id, enabled: !item.enabled }); await load() } catch (e) { setErr(String(e)) }
  }

  const saveConfig = async () => {
    setBusy(true); setErr(''); setMsg('')
    try {
      const c = await invokeDesktop('set_auto_scheduler_config', { config: { intervalSeconds: Number(config.intervalSeconds), enabled: config.enabled } })
      setConfig({ intervalSeconds: c.intervalSeconds, enabled: c.enabled })
      setMsg('Đã lưu cấu hình scheduler')
    } catch (e) { setErr(String(e)) } finally { setBusy(false) }
  }

  return <section className="media-page">
    <div className="page-heading"><div><p>LỊCH AI CONTENT</p><h1>Lịch AI content</h1><span>Tự động tạo nội dung theo lịch và lưu vào Kho nội dung.</span></div><div className="media-actions"><button className="primary" onClick={openCreate} disabled={!desktop}><Plus size={16}/> Tạo lịch AI</button><button className="outline" onClick={load} disabled={!desktop || busy}><RefreshCw size={16}/> Làm mới</button></div></div>
    {!desktop && <div className="desktop-notice"><HardDrive size={23}/><div><strong>Hãy mở bằng ứng dụng FlowPost AI Desktop</strong><span>Chỉ bản Tauri mới đọc được lịch.</span></div></div>}
    {err && <div className="error-box">{err}</div>}
    {msg && <div className="desktop-notice" style={{background:'#e8f5e9'}}><CircleCheck size={20}/><div><strong>{msg}</strong></div></div>}

    <div className="panel" style={{padding:14, marginBottom:14}}>
      <strong style={{fontSize:14}}>Cấu hình scheduler</strong>
      <div style={{display:'flex', gap:10, alignItems:'center', marginTop:8, flexWrap:'wrap'}}>
        <label style={{fontSize:13}}>Khoảng kiểm tra (giây): <input type="number" value={config.intervalSeconds} onChange={e=>setConfig(c=>({...c, intervalSeconds: e.target.value}))} style={{width:80, height:32, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px'}}/></label>
        <label style={{fontSize:13, display:'flex', alignItems:'center', gap:6}}><input type="checkbox" checked={config.enabled} onChange={e=>setConfig(c=>({...c, enabled: e.target.checked}))}/> Bật scheduler</label>
        <button className="primary" onClick={saveConfig} disabled={!desktop || busy} style={{height:32}}>Lưu cấu hình</button>
      </div>
    </div>

    <div className="panel" style={{padding:0, overflow:'hidden'}}>
      {items.length ? <div className="table-wrap"><table><thead><tr><th>TÊN</th><th>LỊCH</th><th>TRẠNG THÁI</th><th>LẦN CHẠY CUỐI</th><th>LỖI</th><th></th></tr></thead><tbody>{items.map(item => (
        <tr key={item.id}>
          <td><strong style={{fontSize:14}}>{item.name}</strong></td>
          <td>{recurrenceDisplay(item.recurrence)}</td>
          <td><button className="outline" onClick={()=>toggle(item)} style={{height:28, fontSize:12}}>{item.enabled ? 'Bật' : 'Tắt'}</button></td>
          <td style={{fontSize:12}}>{item.lastRunAt ? new Date(item.lastRunAt).toLocaleString('vi-VN') : '—'}</td>
          <td style={{fontSize:12, color:'#c62828', maxWidth:200, wordBreak:'break-word'}}>{item.lastError || '—'}</td>
          <td><div style={{display:'flex', gap:4}}><button className="outline" onClick={()=>openEdit(item)} style={{height:30, fontSize:12}}><Pencil size={12}/> Sửa</button><button className="outline" onClick={()=>remove(item.id)} style={{height:30, fontSize:12}}><Trash2 size={12}/> Xóa</button></div></td>
        </tr>
      ))}</tbody></table></div> : <div className="media-empty" style={{padding:30}}><div><Bot size={34}/></div><h2>Chưa có lịch AI nào</h2><p>Tạo lịch AI để tự động sinh nội dung.</p></div>}
    </div>

    {showModal && <div style={{position:'fixed', inset:0, background:'#0006', display:'grid', placeItems:'center', zIndex:55}} onClick={()=>setShowModal(false)}>
      <div className="panel" style={{width:520, padding:16, display:'flex', flexDirection:'column', gap:10, maxHeight:'90vh', overflow:'auto'}} onClick={e=>e.stopPropagation()}>
        <h3 style={{margin:0, fontSize:15}}>{editing ? 'Sửa lịch AI' : 'Tạo lịch AI'}</h3>
        <label style={{fontSize:13}}>Tên
          <input value={form.name} onChange={e=>setForm(f=>({...f, name: e.target.value}))} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:4}}/>
        </label>
        <label style={{fontSize:13}}>Loại lịch
          <select value={form.recurrenceType} onChange={e=>setForm(f=>({...f, recurrenceType: e.target.value}))} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:4}}>
            <option value="fixed">Giờ cố định</option>
            <option value="interval">Khoảng thời gian</option>
          </select>
        </label>
        {form.recurrenceType === 'fixed' ? (
          <label style={{fontSize:13}}>Giờ (HH:MM, cách nhau dấu phẩy)
            <input value={form.fixedTimes} onChange={e=>setForm(f=>({...f, fixedTimes: e.target.value}))} placeholder="08:00, 10:00" style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:4}}/>
          </label>
        ) : (
          <label style={{fontSize:13}}>Khoảng (phút)
            <input type="number" value={form.intervalMinutes} onChange={e=>setForm(f=>({...f, intervalMinutes: e.target.value}))} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:4}}/>
          </label>
        )}
        <label style={{fontSize:13}}>Prompt
          <textarea value={form.prompt} onChange={e=>setForm(f=>({...f, prompt: e.target.value}))} rows={4} style={{width:'100%', border:'1px solid #e2e1e7', borderRadius:8, padding:'8px', marginTop:4}}/>
        </label>
        <label style={{fontSize:13}}>Style
          <select value={form.style} onChange={e=>setForm(f=>({...f, style: e.target.value}))} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:4}}>
            {styles.map(s => <option key={s.id || s.name} value={s.name || s.id}>{s.name || s.id}</option>)}
          </select>
        </label>
        <label style={{fontSize:13}}>Provider (tùy chọn)
          <select value={form.providerId} onChange={e=>setForm(f=>({...f, providerId: e.target.value}))} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:4}}>
            <option value="">-- Mặc định --</option>
            {providers.map(p => <option key={p.id} value={p.id}>{p.displayName || p.id}</option>)}
          </select>
        </label>
        <label style={{fontSize:13}}>Model (tùy chọn)
          <input value={form.model} onChange={e=>setForm(f=>({...f, model: e.target.value}))} style={{width:'100%', height:36, border:'1px solid #e2e1e7', borderRadius:8, padding:'0 8px', marginTop:4}}/>
        </label>
        <label style={{fontSize:13, display:'flex', alignItems:'center', gap:6}}><input type="checkbox" checked={form.autoApprove} onChange={e=>setForm(f=>({...f, autoApprove: e.target.checked}))}/> Tự động duyệt</label>
        {editing && <label style={{fontSize:13, display:'flex', alignItems:'center', gap:6}}><input type="checkbox" checked={form.enabled} onChange={e=>setForm(f=>({...f, enabled: e.target.checked}))}/> Bật</label>}
        {err && <div className="error-box">{err}</div>}
        <div style={{display:'flex', justifyContent:'flex-end', gap:8}}>
          <button className="outline" onClick={()=>setShowModal(false)} disabled={busy} style={{height:32}}>Đóng</button>
          <button className="primary" onClick={save} disabled={busy} style={{height:32}}>{editing ? 'Cập nhật' : 'Tạo'}</button>
        </div>
      </div>
    </div>}
  </section>
}

export default App
