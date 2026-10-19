/* Shared chrome for the documentation pages: header, sidebar, on-page contents, pager and
   footer are built here so every page only has to carry its own <article class="doc">. */
(function () {
  var GITHUB = 'https://github.com/Mio888888/Mio-Music';
  var PLUGINS_REPO = 'https://github.com/Mio888888/mio-music-plugins';

  // Paths are relative to /docs/. Edit this list to add a page.
  var NAV = [
    { title: '开始', items: [
      { title: '文档首页', href: 'index.html' },
      { title: '快速开始', href: 'getting-started.html' },
      { title: '使用指南', href: 'user-guide.html' },
      { title: '架构概览', href: 'architecture.html' }
    ] },
    { title: '插件开发', items: [
      { title: '开发概览', href: 'plugins/index.html' },
      { title: '快速上手', href: 'plugins/quickstart.html' },
      { title: '列表插件参考', href: 'plugins/list-plugin.html' },
      { title: '数据结构', href: 'plugins/data-types.html' },
      { title: '运行时 API', href: 'plugins/runtime-api.html' },
      { title: '音源插件', href: 'plugins/playback-plugin.html' },
      { title: '调试与发布', href: 'plugins/publishing.html' }
    ] }
  ];

  var body = document.body;
  var article = document.querySelector('article.doc');
  if (!article) return;

  // body[data-root] is the path from this page back to the site root, e.g. "../" or "../../".
  var root = body.getAttribute('data-root') || './';
  var docsBase = root + 'docs/';
  var here = body.getAttribute('data-page') || '';

  var flat = [];
  NAV.forEach(function (group) { group.items.forEach(function (item) { flat.push(item); }); });
  var index = flat.findIndex(function (item) { return item.href === here; });

  function el(tag, attrs, html) {
    var node = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (key) { node.setAttribute(key, attrs[key]); });
    if (html != null) node.innerHTML = html;
    return node;
  }
  function icon(name, size) {
    return '<iconify-icon icon="' + name + '" width="' + (size || 14) + '"></iconify-icon>';
  }

  /* ---------- header ---------- */
  var header = el('header', { class: 'docs-header' },
    '<div class="docs-header-inner">' +
      '<button type="button" class="docs-menu-btn" aria-label="打开目录" aria-expanded="false" data-docs-menu>' +
        '<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M4 7h16M4 12h16M4 17h16"/></svg>' +
      '</button>' +
      '<a href="' + root + '" class="docs-brand"><img src="' + root + 'assets/app-icon.png" alt="Mio Music" width="28" height="28"><span>Mio Music</span></a>' +
      '<span class="docs-brand-sep" aria-hidden="true">/</span>' +
      '<a href="' + docsBase + '" class="docs-brand-doc">文档</a>' +
      '<nav class="docs-top-links" aria-label="站点">' +
        '<a href="' + root + '">官网</a>' +
        '<a href="' + docsBase + 'plugins/index.html">插件开发</a>' +
        '<a href="' + GITHUB + '" target="_blank" rel="noreferrer">' + icon('solar:star-linear', 14) + 'GitHub</a>' +
      '</nav>' +
    '</div>');

  /* ---------- sidebar ---------- */
  var sidebar = el('aside', { class: 'docs-sidebar', 'aria-label': '文档目录' });
  NAV.forEach(function (group) {
    var block = el('div', { class: 'docs-nav-group' });
    block.appendChild(el('div', { class: 'docs-nav-title' }, group.title));
    group.items.forEach(function (item) {
      var link = el('a', { href: docsBase + item.href, class: 'docs-nav-link' + (item.href === here ? ' is-active' : '') }, item.title);
      if (item.href === here) link.setAttribute('aria-current', 'page');
      block.appendChild(link);
    });
    sidebar.appendChild(block);
  });
  var extra = el('div', { class: 'docs-nav-group' });
  extra.appendChild(el('div', { class: 'docs-nav-title' }, '相关链接'));
  extra.appendChild(el('a', { href: GITHUB, target: '_blank', rel: 'noreferrer', class: 'docs-nav-link' }, 'GitHub 仓库'));
  extra.appendChild(el('a', { href: PLUGINS_REPO, target: '_blank', rel: 'noreferrer', class: 'docs-nav-link' }, '内置插件仓库'));
  extra.appendChild(el('a', { href: GITHUB + '/issues', target: '_blank', rel: 'noreferrer', class: 'docs-nav-link' }, '提交 Issue'));
  sidebar.appendChild(extra);

  /* ---------- headings, anchors, tables, code ---------- */
  var headings = [];
  var used = {};
  Array.prototype.forEach.call(article.querySelectorAll('h2, h3'), function (h) {
    var id = h.id;
    if (!id) {
      id = h.textContent.trim().toLowerCase().replace(/[^\w一-鿿]+/g, '-').replace(/^-+|-+$/g, '') || 'section';
      while (used[id]) id += '-x';
      h.id = id;
    }
    used[id] = true;
    h.insertAdjacentHTML('beforeend', ' <a class="doc-anchor" href="#' + id + '" aria-label="链接到此节">#</a>');
    headings.push(h);
  });

  Array.prototype.forEach.call(article.querySelectorAll('table'), function (table) {
    var wrap = el('div', { class: 'doc-table-wrap' });
    table.parentNode.insertBefore(wrap, table);
    wrap.appendChild(table);
  });

  function esc(text) { return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }
  var JS_KEYWORDS = /^(?:async|await|break|case|catch|const|continue|default|else|export|false|finally|for|function|if|in|let|new|null|of|return|switch|throw|true|try|typeof|undefined|var|while)$/;
  // A small tokenizer: comments, strings, numbers and keywords. It never reorders text.
  function highlightJs(code) {
    var re = /(\/\/[^\n]*|\/\*[\s\S]*?\*\/)|(`(?:\\[\s\S]|[^`\\])*`|'(?:\\.|[^'\\\n])*'|"(?:\\.|[^"\\\n])*")|(\b\d[\d_.]*\b)|([A-Za-z_$][\w$]*)|([\s\S])/g;
    var out = '';
    var m;
    while ((m = re.exec(code))) {
      if (m[1]) out += '<span class="tk-c">' + esc(m[1]) + '</span>';
      else if (m[2]) out += '<span class="tk-s">' + esc(m[2]) + '</span>';
      else if (m[3]) out += '<span class="tk-n">' + esc(m[3]) + '</span>';
      else if (m[4]) out += JS_KEYWORDS.test(m[4]) ? '<span class="tk-k">' + esc(m[4]) + '</span>' : esc(m[4]);
      else out += esc(m[5]);
    }
    return out;
  }
  function highlightShell(code) {
    return code.split('\n').map(function (line) {
      if (/^\s*#/.test(line)) return '<span class="tk-c">' + esc(line) + '</span>';
      return esc(line);
    }).join('\n');
  }

  Array.prototype.forEach.call(article.querySelectorAll('pre'), function (pre) {
    var code = pre.querySelector('code') || pre;
    var lang = ((code.className + ' ' + pre.className).match(/language-(\w+)/) || [])[1] || '';
    var text = code.textContent.replace(/^\n/, '').replace(/\s+$/, '');
    if (lang === 'js' || lang === 'json') code.innerHTML = highlightJs(text);
    else if (lang === 'bash' || lang === 'sh') code.innerHTML = highlightShell(text);
    else code.textContent = text;
    var box = el('div', { class: 'doc-code' });
    var label = pre.getAttribute('data-title');
    pre.parentNode.insertBefore(box, pre);
    if (label) box.appendChild(el('div', { class: 'doc-code-title' }, esc(label)));
    box.appendChild(pre);
    var button = el('button', { type: 'button', class: 'doc-copy', 'aria-label': '复制代码' }, '复制');
    button.addEventListener('click', function () {
      var done = function () {
        button.textContent = '已复制';
        setTimeout(function () { button.textContent = '复制'; }, 1400);
      };
      if (navigator.clipboard && navigator.clipboard.writeText) navigator.clipboard.writeText(text).then(done, function () {});
      else {
        var area = el('textarea', { style: 'position:fixed;opacity:0' });
        area.value = text;
        document.body.appendChild(area);
        area.select();
        try { document.execCommand('copy'); done(); } catch (e) { /* ignore */ }
        document.body.removeChild(area);
      }
    });
    box.appendChild(button);
  });

  /* ---------- on-page contents ---------- */
  var toc = el('nav', { class: 'docs-toc', 'aria-label': '本页内容' });
  if (headings.length > 1) {
    toc.appendChild(el('div', { class: 'docs-toc-title' }, '本页内容'));
    headings.forEach(function (h) {
      var link = el('a', { href: '#' + h.id, class: 'docs-toc-link' + (h.tagName === 'H3' ? ' is-sub' : '') });
      link.textContent = h.textContent.replace(/\s*#$/, '');
      toc.appendChild(link);
    });
  }

  /* ---------- pager ---------- */
  var pager = el('nav', { class: 'doc-pager', 'aria-label': '上一页与下一页' });
  if (index > 0) pager.appendChild(el('a', { href: docsBase + flat[index - 1].href, class: 'is-prev' }, '<span>上一页</span><strong>' + flat[index - 1].title + '</strong>'));
  if (index >= 0 && index < flat.length - 1) pager.appendChild(el('a', { href: docsBase + flat[index + 1].href, class: 'is-next' }, '<span>下一页</span><strong>' + flat[index + 1].title + '</strong>'));

  var edit = el('div', { class: 'doc-edit' },
    '<a href="' + GITHUB + '/edit/main/website/docs/' + here + '" target="_blank" rel="noreferrer">' + icon('solar:code-square-linear', 14) + '在 GitHub 上编辑此页</a>');

  /* ---------- assemble ---------- */
  var footer = el('footer', { class: 'docs-footer' },
    '<span>© 2026 Mio Music · 开源免费</span>' +
    '<span><a href="' + root + '">官网</a> · <a href="' + GITHUB + '" target="_blank" rel="noreferrer">GitHub</a> · <a href="' + PLUGINS_REPO + '" target="_blank" rel="noreferrer">插件仓库</a></span>');

  var main = el('main', { class: 'docs-main', id: 'docs-main' });
  article.parentNode.insertBefore(main, article);
  main.appendChild(article);
  main.appendChild(edit);
  main.appendChild(pager);

  var shell = el('div', { class: 'docs-shell' });
  var backdrop = el('div', { class: 'docs-backdrop', 'data-docs-backdrop': '' });
  shell.appendChild(sidebar);
  shell.appendChild(main);
  shell.appendChild(toc);

  var bg = el('div', { class: 'docs-bg', 'aria-hidden': 'true' });
  body.insertBefore(bg, body.firstChild);
  body.insertBefore(header, bg.nextSibling);
  body.insertBefore(shell, header.nextSibling);
  body.insertBefore(backdrop, shell.nextSibling);
  body.appendChild(footer);

  /* ---------- drawer (small screens) ---------- */
  var menuButton = header.querySelector('[data-docs-menu]');
  function setDrawer(open) {
    body.classList.toggle('docs-drawer-open', open);
    menuButton.setAttribute('aria-expanded', open ? 'true' : 'false');
  }
  menuButton.addEventListener('click', function () { setDrawer(!body.classList.contains('docs-drawer-open')); });
  backdrop.addEventListener('click', function () { setDrawer(false); });
  sidebar.addEventListener('click', function (event) { if (event.target.closest('a')) setDrawer(false); });
  document.addEventListener('keydown', function (event) { if (event.key === 'Escape') setDrawer(false); });

  /* ---------- scroll spy ---------- */
  var tocLinks = Array.prototype.slice.call(toc.querySelectorAll('.docs-toc-link'));
  if (tocLinks.length && 'IntersectionObserver' in window) {
    var byId = {};
    tocLinks.forEach(function (link) { byId[link.getAttribute('href').slice(1)] = link; });
    var observer = new IntersectionObserver(function (entries) {
      entries.forEach(function (entry) {
        if (!entry.isIntersecting) return;
        tocLinks.forEach(function (link) { link.classList.remove('is-active'); });
        var link = byId[entry.target.id];
        if (link) link.classList.add('is-active');
      });
    }, { rootMargin: '-72px 0px -70% 0px' });
    headings.forEach(function (h) { observer.observe(h); });
    // A short last section never reaches the observer band; pick it once the page ends.
    window.addEventListener('scroll', function () {
      if (window.innerHeight + window.scrollY < document.documentElement.scrollHeight - 4) return;
      tocLinks.forEach(function (link) { link.classList.remove('is-active'); });
      tocLinks[tocLinks.length - 1].classList.add('is-active');
    }, { passive: true });
  }

  // The injected headings did not exist when the browser first looked for the hash target.
  if (location.hash.length > 1) {
    var target = document.getElementById(decodeURIComponent(location.hash.slice(1)));
    if (target) target.scrollIntoView();
  }
})();
