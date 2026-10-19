// Paste in the original Tauri developer console after detaching the inspector.
// The collector only accepts data from this localhost origin. No input values are captured.
(async () => {
  const selectors = {sidebar:'.sidebar',brand:'.logo-section',header:'.header',search:'.search-input',guide:'.setup-guide',orb:'.setup-orb',copy:'.setup-copy',title:'.setup-copy h3',description:'.setup-copy p',cards:'.setup-actions',primary:'.primary-card',card:'.setup-action-card:not(.primary-card)',player:'.player-container',mobileNav:'.mobile-bottom-nav'};
  const properties = ['color','backgroundColor','backgroundImage','fontFamily','fontSize','fontWeight','lineHeight','borderRadius','borderColor','boxShadow'];
  const elements = Object.fromEntries(Object.entries(selectors).map(([id,selector])=>{
    const node=document.querySelector(selector);if(!node)return [id,null];
    const rect=node.getBoundingClientRect(),style=getComputedStyle(node);
    return [id,{selector,bounds:{x:rect.x,y:rect.y,width:rect.width,height:rect.height},style:Object.fromEntries(properties.map(p=>[p,style[p]]))}];
  }));
  const data={route:location.hash,theme:document.documentElement.dataset.theme||'light',variant:document.documentElement.dataset.theme==='dark'?'css-dark-probe':'original-idle',viewport:{width:innerWidth,height:innerHeight,dpr:devicePixelRatio},elements};
  const response=await fetch('http://127.0.0.1:1421',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(data)});
  console.log('Reference captured',response.status);return data;
})();
