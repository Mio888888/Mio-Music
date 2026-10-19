// Paste in the original desktop WebView developer console on the desired route.
// Read-only: downloads a spec; it does not change state or install application hooks.
(() => {
  const properties = ['color', 'backgroundColor', 'backgroundImage', 'fontFamily', 'fontSize', 'fontWeight', 'lineHeight', 'letterSpacing', 'borderRadius', 'borderColor', 'borderWidth', 'boxShadow', 'opacity', 'backdropFilter', 'transition', 'animation', 'overflow', 'textOverflow'];
  const elements = [];
  const visit = (node, path) => {
    const rect = node.getBoundingClientRect();
    const style = getComputedStyle(node);
    if (rect.width && rect.height && style.display !== 'none' && style.visibility !== 'hidden') {
      elements.push({ path, tag: node.tagName, id: node.id, className: node.getAttribute('class'),
        text: [...node.childNodes].filter(n => n.nodeType === Node.TEXT_NODE).map(n => n.textContent).join('').trim(),
        bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
        style: Object.fromEntries(properties.map(key => [key, style[key]])) });
    }
    [...node.children].forEach((child, index) => visit(child, `${path}/${index}`));
  };
  visit(document.querySelector('#app'), 'app');
  const style = getComputedStyle(document.documentElement);
  const tokens = Object.fromEntries([...style].filter(key => key.startsWith('--')).map(key => [key, style.getPropertyValue(key).trim()]));
  const spec = { route: location.hash, viewport: { width: innerWidth, height: innerHeight, dpr: devicePixelRatio }, theme: document.documentElement.dataset.theme || 'light', tokens, elements };
  const url = URL.createObjectURL(new Blob([JSON.stringify(spec, null, 2)], { type: 'application/json' }));
  const a = document.createElement('a'); a.href = url; a.download = `mio-${spec.theme}-${innerWidth}x${innerHeight}-${location.hash.replace(/[^a-z0-9]/gi, '_')}.json`; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  return spec;
})();
