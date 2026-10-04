import React, {useState} from 'react';
import {createRoot} from 'react-dom/client';
import i18next from 'i18next';
import {initReactI18next} from 'react-i18next';
import zh from '@/i18n/locales/zh.json';
import '@/index.css';
import {AggregateProviderFields} from '@/components/providers/forms/AggregateProviderFields';
import {CodexAggregateFields} from '@/components/providers/forms/CodexAggregateFields';
import type {Provider, AggregateRoutes, CodexAggregateRoutes} from '@/types';
void i18next.use(initReactI18next).init({lng:'zh', resources:{zh:{translation:zh}}, interpolation:{escapeValue:false}});
const candidates: Provider[] = [
 {id:'demo-a',name:'模拟供应商 A',settingsConfig:{}},
 {id:'demo-b',name:'模拟供应商 B',settingsConfig:{}},
];
const claudeSeed: AggregateRoutes = {slots:[
 {routeId:'claude-sonnet-4-6',tier:'sonnet',providerId:'demo-a',upstreamModel:'mock-reasoner-a',label:'供应商 A · 模拟推理模型'},
 {routeId:'claude-sonnet-4-5',tier:'sonnet',providerId:'demo-b',upstreamModel:'mock-reasoner-b',label:'供应商 B · 模拟推理模型'},
],defaultTarget:{kind:'slotId',value:'claude-sonnet-4-6'},defaultModel:'claude-sonnet-4-6'};
const codexSeed: CodexAggregateRoutes = {slots:[
 {model:'team-smart',providerId:'demo-a',upstreamModel:'mock-reasoner-a',label:'供应商 A · 推理'},
 {model:'team-fast',providerId:'demo-b',upstreamModel:'mock-fast-b',label:'供应商 B · 快速'},
],defaultTarget:{kind:'slotId',value:'team-smart'},defaultModel:'team-smart'};
function App(){
 const [mode,setMode]=useState('claude'); const [claude,setClaude]=useState(claudeSeed); const [codex,setCodex]=useState(codexSeed);
 const reset=()=>{setClaude({...claudeSeed,slots:[]});setCodex({...codexSeed,slots:[]});};
 const common={candidates,modelsForProvider:()=>[{id:'mock-reasoner-a',ownedBy:null},{id:'mock-fast-b',ownedBy:null}],fetchingProviderId:null,onFetchModels:()=>{}};
 return <main style={{maxWidth:850,margin:'24px auto',padding:24}}><h1 style={{fontSize:22,marginBottom:10}}>聚合功能 · 隔离评估</h1>
 <p style={{fontSize:13,marginBottom:14}}>真实界面组件 + 模拟数据；不连接账号、原生应用或上游接口。以下三个按钮为评估工具。</p>
 <nav style={{display:'flex',gap:16,marginBottom:24}}><button onClick={()=>setMode('claude')}>查看 Claude 聚合</button><button onClick={()=>setMode('codex')}>查看 Codex 聚合</button><button onClick={reset}>清空测试槽位</button></nav>
 {mode==='claude'?<AggregateProviderFields value={claude} onChange={setClaude} {...common}/>:<CodexAggregateFields value={codex} onChange={setCodex} {...common}/>}
 <details style={{marginTop:24}}><summary>模拟提交数据（评估工具）</summary><pre style={{fontSize:12,whiteSpace:'pre-wrap'}}>{JSON.stringify(mode==='claude'?claude:codex,null,2)}</pre></details></main>
}
createRoot(document.getElementById('root')!).render(<App/>);
