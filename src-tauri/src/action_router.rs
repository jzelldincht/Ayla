//! action_router：五动作统一入口（selection_translate/input_translate/ocr_recognize/ocr_translate/toggle_scene）
//! 与前置检查链（挂起队列、hasKey、引导/框选压制）—— T16 实现；依赖方向 action_router→能力模块→config/events（spec 5.4）。
