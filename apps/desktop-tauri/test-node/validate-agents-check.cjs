'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
async function scene(band, late = false) {
  let current = null, changed = false, instructions = '';
  const element = (id, textContent = '') => ({id, textContent, disabled:false, value:'',
    classList:{contains:()=>true}, scrollIntoView(){}, closest:()=>null,
    getBoundingClientRect:()=>({x:id==='tab'?10:30,y:0,width:10,height:10})});
  const tab = element('tab'), toggle = element('toggle'), save = element('save','Guardar instrucciones');
  const field = element('abito-instructions');
  const root = {get textContent() {return `Agentes. Se aplican a los próximos mensajes del chat de Abito. ${!late || changed ? band : ''}`;},
    scrollIntoView(){}, querySelector:()=>toggle, querySelectorAll:selector=>selector==='button'?[save]:[toggle,save,field]};
  const window = {__validateOrigin:{x:0,y:0,scale:1},__TAURI__:{core:{invoke:async(command)=>
    command==='prefs_action'?{changed,abitoInstructions:instructions}:{prefs:{}}},event:{emit:async()=>{
      if (!current) current=root;
      else {changed=true; instructions=field.value;}
    }}}};
  vm.runInNewContext(fs.readFileSync(path.join(__dirname,'../packaging/validate/browser.js'),'utf8'),{
    window, document:{querySelector:()=>current,querySelectorAll:()=>[tab],getElementById:()=>field},
    addEventListener(){},console:{error(){}},performance,
    setTimeout:fn=>{fn();return 1;},requestAnimationFrame:fn=>fn(),
    getComputedStyle:()=>({display:'block',visibility:'visible'})});
  return window.AbitoValidate.hub(0,'integrations');
}
(async()=>{
  for (const band of ['No se pudo completar; no se tocó nada.', 'install_failed', 'No se pudo completar']) {
    for (const late of [false,true]) {
      const result = await scene(band, late);
      assert.equal(result.ok,false,`ajustes-integrations must reject ${band} (late=${late})`);
    }
  }
  assert.equal((await scene('Codex no disponible')).ok,true,'unavailable is not an installer error');
  console.log('validate-agents: 7 scene checks passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
