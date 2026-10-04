import {describe, it, expect} from 'vitest';
import {assignSlotIds, flattenProviderGroups, groupSlotsByProvider} from '../../src/utils/aggregateRoutes';
import type {AggregateRoutes} from '../../src/types';
const routes: AggregateRoutes={slots:[
 {routeId:'claude-sonnet-4-6',tier:'sonnet',providerId:'mock-a',upstreamModel:'upstream-a'},
 {routeId:'claude-sonnet-4-5',tier:'sonnet',providerId:'mock-b',upstreamModel:'upstream-b'},
],defaultTarget:{kind:'slotId',value:'claude-sonnet-4-6'}};
describe('评估所需的兼容性要求（预期当前实现红灯）',()=>{
 it('仅调整显示顺序时，既有会话携带的模型 ID 仍应命中原来的上游',()=>{
  const reordered=assignSlotIds({...routes,slots:[...routes.slots].reverse()});
  expect(reordered.slots.find(s=>s.routeId==='claude-sonnet-4-6')?.upstreamModel).toBe('upstream-a');
 });
 it('编辑器的分组再展平不应静默删除存量中同供应商同档位的第二个模型',()=>{
  const legacy=routes.slots.map(s=>({...s,providerId:'mock-a'}));
  const edited=flattenProviderGroups(groupSlotsByProvider(legacy));
  expect(edited.map(s=>s.upstreamModel)).toEqual(['upstream-a','upstream-b']);
 });
});
