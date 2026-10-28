# 个人漫画图源

用于 Aidoku 的蚂蚁搬运网与 JComic 整站图源，支持浏览、搜索、章节目录与图片读取，不绑定特定漫画。

## 添加到 Aidoku

在 Aidoku → 设置（Settings）→ 图源列表（Source Lists）→ `+` 中添加：

```text
https://raw.githubusercontent.com/kai555ww/aidoku-custom-sources/main/public/index.min.json
```

然后在浏览（Browse）中安装「蚂蚁搬运网」和「JComic」。网站包含成人内容，源清单已相应标记；显示设置可能影响是否可见。

## 实现

- 蚂蚁搬运网：解析网页中的图片数组，合并阅读分页，并设置图片 Referer。
- JComic：支持多章节和单篇作品；每次请求图片或封面时刷新短时签名。
- 依赖固定于 Aidoku SDK `ae6485e3c14ecd1fd5497d170eae884c68d04a87`。
- `public/` 只包含安装包、图标和源清单，不包含漫画图片、登录信息或阅读历史。

## 维护

安装 Rust 1.99.0 与官方 CLI 后运行：

```sh
cargo install --locked --git https://github.com/Aidoku/aidoku-rs --rev ae6485e3c14ecd1fd5497d170eae884c68d04a87 aidoku-cli
bash build_on_mac.sh
```

推送源码到 `main` 后，GitHub Actions 会编译、验证安装包并更新 `public/`。发布更新时需增加对应 `res/source.json` 的版本号。

## 验证

安装包使用官方 CLI 验证。官方 WASM 测试运行器覆盖浏览分页、搜索、章节和图片地址刷新。蚂蚁搬运网首尾图片请求成功。JComic 的 CDN 对 Rust 测试运行器返回 Cloudflare 403；同一章节的 26 张图片在真实浏览器中全部加载成功，macOS 原生 URLSession 也成功取得首尾图片（HTTP 200）。Rust 运行器的图片响应测试单独标为忽略。以上不能替代 Aidoku App 内的实际安装和阅读确认。

开发资料：[官方图源开发](https://aidoku.github.io/aidoku-rs/book/)、[外部图源安装](https://aidoku.app/help/guides/getting-started/)。
