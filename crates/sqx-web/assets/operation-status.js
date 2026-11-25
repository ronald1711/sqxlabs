(function(root){
  'use strict';
  function create(render, clock={now:()=>Date.now(),every:fn=>setInterval(fn,1000),clear:id=>clearInterval(id)}) {
    let sequence=0,timer=null;const records=new Map();
    function draw(){
      const all=[...records.values()];
      const visible=[...all.filter(x=>x.state==='running'),...all.filter(x=>x.state!=='running').slice(-3)];
      render(visible.map(x=>({...x,message:x.detail+' · '+Math.floor(((x.ended ?? clock.now())-x.started)/1000)+'s elapsed'})));
      for(const x of all)if(!visible.includes(x))records.delete(x.id);
      if(!all.some(x=>x.state==='running')&&timer!==null){clock.clear(timer);timer=null;}
    }
    function settle(id,state,detail){const x=records.get(id);if(!x)return;x.state=state;x.detail=detail;x.ended=clock.now();draw();}
    return {
      start(label){const id=++sequence;records.set(id,{id,label,state:'running',detail:'Working… progress percentage unavailable',started:clock.now()});if(timer===null)timer=clock.every(draw);draw();return id;},
      dismissCompleted(){for(const [id,x] of records)if(x.state!=='running')records.delete(id);draw();},
      finish(id,data={}){
        const record=records.get(id);
        if(record&&Number.isFinite(data.completed)&&Number.isFinite(data.total)&&data.total>0&&data.completed>=0&&data.completed<=data.total){record.completed=data.completed;record.total=data.total;}
        if(data.error)return settle(id,'error','Failed: '+String(data.error));
        if(data.recovery_required)return settle(id,'attention','Saved request needs attention — inspect its result / saved research');
        if(data.state==='resumable'||data.state==='paused')return settle(id,'attention','Progress saved — not finished');
        if(Number.isFinite(data.completed)&&Number.isFinite(data.total)&&data.completed<data.total)return settle(id,'attention',data.completed+' / '+data.total+' candidates saved — search continues');
        settle(id,'complete',Array.isArray(data.results)?'Completed · '+data.results.length+' candidates returned':'Completed — see results');
      },
      fail(id,message){settle(id,'error','Response unavailable: '+message+'. The server may still be working; check saved research before retrying.');}
    };
  }
  const api={create};if(typeof module!=='undefined'&&module.exports)module.exports=api;else root.SqxOperationStatus=api;
})(typeof globalThis!=='undefined'?globalThis:this);
