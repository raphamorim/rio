"use strict";(self.webpackChunkrio_docs=self.webpackChunkrio_docs||[]).push([[3955],{6184:(e,o,i)=>{i.r(o),i.d(o,{assets:()=>c,contentTitle:()=>d,default:()=>u,frontMatter:()=>s,metadata:()=>t,toc:()=>a});const t=JSON.parse('{"id":"config/editor","title":"editor","description":"This setting specifies the editor Rio will use to open the configuration file. By default, the editor is set to vi.","source":"@site/docs/config/editor.md","sourceDirName":"config","slug":"/config/editor","permalink":"/rio/ko/docs/config/editor","draft":false,"unlisted":false,"editUrl":"https://github.com/raphamorim/rio/tree/main/docs/docs/config/editor.md","tags":[],"version":"current","frontMatter":{"title":"editor","language":"en"},"sidebar":"tutorialSidebar","previous":{"title":"developer","permalink":"/rio/ko/docs/config/developer"},"next":{"title":"env-vars","permalink":"/rio/ko/docs/config/env-vars"}}');var n=i(4848),r=i(8453);const s={title:"editor",language:"en"},d=void 0,c={},a=[];function l(e){const o={admonition:"admonition",code:"code",p:"p",pre:"pre",...(0,r.R)(),...e.components};return(0,n.jsxs)(n.Fragment,{children:[(0,n.jsxs)(o.p,{children:["This setting specifies the editor Rio will use to open the configuration file. By default, the editor is set to ",(0,n.jsx)(o.code,{children:"vi"}),"."]}),"\n",(0,n.jsxs)(o.p,{children:["Whenever the key binding ",(0,n.jsx)(o.code,{children:"OpenConfigEditor"})," is triggered, Rio will use the configured editor and the path to the Rio configuration file."]}),"\n",(0,n.jsx)(o.p,{children:"For example, if you have VS Code installed and want to use it as your editor, the configuration would look like this:"}),"\n",(0,n.jsx)(o.pre,{children:(0,n.jsx)(o.code,{className:"language-toml",children:'[editor]\nprogram = "code"\nargs = []\n'})}),"\n",(0,n.jsxs)(o.p,{children:["When ",(0,n.jsx)(o.code,{children:"OpenConfigEditor"})," is triggered, it will execute the command:\n",(0,n.jsx)(o.code,{children:"$ code <path-to-rio-configuration-file>"}),"."]}),"\n",(0,n.jsxs)(o.admonition,{type:"warning",children:[(0,n.jsxs)(o.p,{children:["If you set a value for ",(0,n.jsx)(o.code,{children:"program"}),", Rio will look for it in the default system application directory (",(0,n.jsx)(o.code,{children:"/usr/bin"})," on Linux and macOS). If your desired editor is not in this directory, you must specify its full path:"]}),(0,n.jsx)(o.pre,{children:(0,n.jsx)(o.code,{className:"language-toml",children:'[editor]\nprogram = "/usr/local/bin/code"\nargs = []\n'})})]})]})}function u(e={}){const{wrapper:o}={...(0,r.R)(),...e.components};return o?(0,n.jsx)(o,{...e,children:(0,n.jsx)(l,{...e})}):l(e)}},8453:(e,o,i)=>{i.d(o,{R:()=>s,x:()=>d});var t=i(6540);const n={},r=t.createContext(n);function s(e){const o=t.useContext(r);return t.useMemo((function(){return"function"==typeof e?e(o):{...o,...e}}),[o,e])}function d(e){let o;return o=e.disableParentContext?"function"==typeof e.components?e.components(n):e.components||n:s(e.components),t.createElement(r.Provider,{value:o},e.children)}}}]);