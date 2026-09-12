# 仕様

## 概要

ESP32-Pico 搭載し、LEGO に組み込んで動くロボット。
PS4 (DualShock 4) コントローラーを Bluetooth Classic で接続し、
その入力でモーターを操作する。

## ハードウェア

* ATOM Matrix v1.1 または ATOM Lite (ESP2-Pico-D4)
* ATOMIC Motionベース v1.2
* DualShock 4 コントローラ
* 

## 機能要件

* DC モーターとサーボモーターを制御する
* PS4 コントローラーを Bluetooth Classic 経由（HID Host）で接続し、
  スティック・ボタンの入力を取得する

## 未確定の項目

* モーター/サーボの実際の配線（GPIO 番号）。[開発環境](development.md) の
  ピン配置は仮のもので、実機の配線に合わせて変更する
