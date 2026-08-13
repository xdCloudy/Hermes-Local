use std::collections::BTreeMap;

pub const LOCALE_STORAGE_KEY: &str = "hermes.desktop.locale";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locale {
    En,
    Zh,
    ZhHant,
    Ja,
    Ar,
}

impl Locale {
    pub const ALL: [Self; 5] = [Self::En, Self::Zh, Self::ZhHant, Self::Ja, Self::Ar];

    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Zh => "zh",
            Self::ZhHant => "zh-hant",
            Self::Ja => "ja",
            Self::Ar => "ar",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Zh => "简体中文",
            Self::ZhHant => "繁體中文",
            Self::Ja => "日本語",
            Self::Ar => "العربية",
        }
    }

    pub const fn direction(self) -> &'static str {
        if matches!(self, Self::Ar) { "rtl" } else { "ltr" }
    }

    pub fn from_code(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "zh" | "zh-cn" | "zh-hans" => Self::Zh,
            "zh-hant" | "zh-tw" | "zh-hk" => Self::ZhHant,
            "ja" | "ja-jp" => Self::Ja,
            "ar" | "ar-sa" => Self::Ar,
            _ => Self::En,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Message {
    Home,
    Chat,
    Tui,
    WebDashboard,
    Tasks,
    Services,
    Models,
    Profiles,
    Tools,
    Memory,
    Skills,
    Sessions,
    Projects,
    Integrations,
    Benchmarks,
    Security,
    Logs,
    SearchSessions,
    Results,
    Pinned,
    Recent,
    NoPinnedSessions,
    Settings,
    Appearance,
    General,
    Provider,
    Updates,
    About,
    Workspace,
    Files,
    Terminal,
    Review,
    Preview,
    AgentConnected,
    ConnectingToAgent,
    AgentError,
    AgentOffline,
    ActiveModel,
    ModelProvider,
    ActiveRuntimeTasks,
    FindInPage,
    Find,
    PreviousMatch,
    NextMatch,
    CloseFind,
    CommandPalette,
    SearchCommands,
    TypeCommand,
    NoMatchingCommands,
    CommandCentre,
    Navigation,
    View,
    ToggleSidebar,
    ToggleRightSidebar,
    ToggleStatusBar,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    SkipWorkspace,
    PersistentTools,
    HideRightSidebar,
    ToolPanes,
    Open,
    Language,
    Minimize,
    Maximize,
    Close,
    Starting,
    CouldNotStart,
    Retry,
}

impl Message {
    pub const ALL: [Self; 70] = [
        Self::Home, Self::Chat, Self::Tui, Self::WebDashboard, Self::Tasks,
        Self::Services, Self::Models, Self::Profiles, Self::Tools, Self::Memory,
        Self::Skills, Self::Sessions, Self::Projects, Self::Integrations,
        Self::Benchmarks, Self::Security, Self::Logs, Self::SearchSessions,
        Self::Results, Self::Pinned, Self::Recent, Self::NoPinnedSessions,
        Self::Settings, Self::Appearance, Self::General, Self::Provider,
        Self::Updates, Self::About, Self::Workspace, Self::Files, Self::Terminal,
        Self::Review, Self::Preview, Self::AgentConnected, Self::ConnectingToAgent,
        Self::AgentError, Self::AgentOffline, Self::ActiveModel, Self::ModelProvider,
        Self::ActiveRuntimeTasks, Self::FindInPage, Self::Find, Self::PreviousMatch,
        Self::NextMatch, Self::CloseFind, Self::CommandPalette, Self::SearchCommands,
        Self::TypeCommand, Self::NoMatchingCommands, Self::CommandCentre,
        Self::Navigation, Self::View, Self::ToggleSidebar, Self::ToggleRightSidebar,
        Self::ToggleStatusBar, Self::ZoomIn, Self::ZoomOut, Self::ResetZoom,
        Self::SkipWorkspace, Self::PersistentTools, Self::HideRightSidebar,
        Self::ToolPanes, Self::Open, Self::Language, Self::Minimize, Self::Maximize,
        Self::Close, Self::Starting, Self::CouldNotStart, Self::Retry,
    ];
}

pub const fn translate(locale: Locale, message: Message) -> &'static str {
    match locale {
        Locale::En => english(message),
        Locale::Zh => simplified_chinese(message),
        Locale::ZhHant => traditional_chinese(message),
        Locale::Ja => japanese(message),
        Locale::Ar => arabic(message),
    }
}

const fn english(message: Message) -> &'static str {
    match message {
        Message::Home => "Home", Message::Chat => "Chat", Message::Tui => "TUI",
        Message::WebDashboard => "Web Dashboard", Message::Tasks => "Tasks",
        Message::Services => "Services", Message::Models => "Models", Message::Profiles => "Profiles",
        Message::Tools => "Tools", Message::Memory => "Memory", Message::Skills => "Skills",
        Message::Sessions => "Sessions", Message::Projects => "Projects", Message::Integrations => "Integrations",
        Message::Benchmarks => "Benchmarks", Message::Security => "Security", Message::Logs => "Logs",
        Message::SearchSessions => "Search sessions", Message::Results => "RESULTS", Message::Pinned => "PINNED",
        Message::Recent => "RECENT", Message::NoPinnedSessions => "No pinned sessions", Message::Settings => "Settings",
        Message::Appearance => "Appearance", Message::General => "General", Message::Provider => "Provider",
        Message::Updates => "Updates", Message::About => "About", Message::Workspace => "Workspace",
        Message::Files => "Files", Message::Terminal => "Terminal", Message::Review => "Review", Message::Preview => "Preview",
        Message::AgentConnected => "Agent connected", Message::ConnectingToAgent => "Connecting to Agent",
        Message::AgentError => "Agent error", Message::AgentOffline => "Agent offline", Message::ActiveModel => "Active model",
        Message::ModelProvider => "Model provider", Message::ActiveRuntimeTasks => "Active runtime tasks",
        Message::FindInPage => "Find in page", Message::Find => "Find", Message::PreviousMatch => "Previous match",
        Message::NextMatch => "Next match", Message::CloseFind => "Close find", Message::CommandPalette => "Command palette",
        Message::SearchCommands => "Search commands", Message::TypeCommand => "Type a command",
        Message::NoMatchingCommands => "No matching commands", Message::CommandCentre => "Command Centre",
        Message::Navigation => "Navigation", Message::View => "View", Message::ToggleSidebar => "Toggle Sidebar",
        Message::ToggleRightSidebar => "Toggle Right Sidebar", Message::ToggleStatusBar => "Toggle Status Bar",
        Message::ZoomIn => "Zoom In", Message::ZoomOut => "Zoom Out", Message::ResetZoom => "Reset Zoom to 90%",
        Message::SkipWorkspace => "Skip to workspace", Message::PersistentTools => "Persistent tools",
        Message::HideRightSidebar => "Hide right sidebar", Message::ToolPanes => "Tool panes", Message::Open => "Open",
        Message::Language => "Language", Message::Minimize => "Minimize", Message::Maximize => "Maximize",
        Message::Close => "Close", Message::Starting => "Starting Hermes Local",
        Message::CouldNotStart => "Hermes Local could not start", Message::Retry => "Retry",
    }
}

const fn simplified_chinese(message: Message) -> &'static str {
    match message {
        Message::Home => "首页", Message::Chat => "聊天", Message::Tui => "终端界面",
        Message::WebDashboard => "网页仪表板", Message::Tasks => "任务", Message::Services => "服务",
        Message::Models => "模型", Message::Profiles => "配置", Message::Tools => "工具", Message::Memory => "记忆",
        Message::Skills => "技能", Message::Sessions => "会话", Message::Projects => "项目", Message::Integrations => "集成",
        Message::Benchmarks => "基准测试", Message::Security => "安全", Message::Logs => "日志",
        Message::SearchSessions => "搜索会话", Message::Results => "结果", Message::Pinned => "已固定", Message::Recent => "最近",
        Message::NoPinnedSessions => "没有固定的会话", Message::Settings => "设置", Message::Appearance => "外观",
        Message::General => "常规", Message::Provider => "提供商", Message::Updates => "更新", Message::About => "关于",
        Message::Workspace => "工作区", Message::Files => "文件", Message::Terminal => "终端", Message::Review => "审查",
        Message::Preview => "预览", Message::AgentConnected => "Agent 已连接", Message::ConnectingToAgent => "正在连接 Agent",
        Message::AgentError => "Agent 错误", Message::AgentOffline => "Agent 离线", Message::ActiveModel => "当前模型",
        Message::ModelProvider => "模型提供商", Message::ActiveRuntimeTasks => "运行中的任务", Message::FindInPage => "在页面中查找",
        Message::Find => "查找", Message::PreviousMatch => "上一个匹配", Message::NextMatch => "下一个匹配",
        Message::CloseFind => "关闭查找", Message::CommandPalette => "命令面板", Message::SearchCommands => "搜索命令",
        Message::TypeCommand => "输入命令", Message::NoMatchingCommands => "没有匹配的命令", Message::CommandCentre => "命令中心",
        Message::Navigation => "导航", Message::View => "视图", Message::ToggleSidebar => "切换侧栏",
        Message::ToggleRightSidebar => "切换右侧栏", Message::ToggleStatusBar => "切换状态栏", Message::ZoomIn => "放大",
        Message::ZoomOut => "缩小", Message::ResetZoom => "重置缩放为 90%", Message::SkipWorkspace => "跳到工作区",
        Message::PersistentTools => "常驻工具", Message::HideRightSidebar => "隐藏右侧栏", Message::ToolPanes => "工具窗格",
        Message::Open => "打开", Message::Language => "语言", Message::Minimize => "最小化", Message::Maximize => "最大化",
        Message::Close => "关闭", Message::Starting => "正在启动 Hermes Local", Message::CouldNotStart => "Hermes Local 无法启动",
        Message::Retry => "重试",
    }
}

const fn traditional_chinese(message: Message) -> &'static str {
    match message {
        Message::Home => "首頁", Message::Chat => "聊天", Message::Tui => "終端介面",
        Message::WebDashboard => "網頁儀表板", Message::Tasks => "任務", Message::Services => "服務",
        Message::Models => "模型", Message::Profiles => "設定檔", Message::Tools => "工具", Message::Memory => "記憶",
        Message::Skills => "技能", Message::Sessions => "工作階段", Message::Projects => "專案", Message::Integrations => "整合",
        Message::Benchmarks => "基準測試", Message::Security => "安全性", Message::Logs => "記錄",
        Message::SearchSessions => "搜尋工作階段", Message::Results => "結果", Message::Pinned => "已釘選", Message::Recent => "最近",
        Message::NoPinnedSessions => "沒有釘選的工作階段", Message::Settings => "設定", Message::Appearance => "外觀",
        Message::General => "一般", Message::Provider => "提供者", Message::Updates => "更新", Message::About => "關於",
        Message::Workspace => "工作區", Message::Files => "檔案", Message::Terminal => "終端機", Message::Review => "審查",
        Message::Preview => "預覽", Message::AgentConnected => "Agent 已連線", Message::ConnectingToAgent => "正在連線 Agent",
        Message::AgentError => "Agent 錯誤", Message::AgentOffline => "Agent 離線", Message::ActiveModel => "目前模型",
        Message::ModelProvider => "模型提供者", Message::ActiveRuntimeTasks => "執行中的任務", Message::FindInPage => "在頁面中尋找",
        Message::Find => "尋找", Message::PreviousMatch => "上一個符合項", Message::NextMatch => "下一個符合項",
        Message::CloseFind => "關閉尋找", Message::CommandPalette => "命令面板", Message::SearchCommands => "搜尋命令",
        Message::TypeCommand => "輸入命令", Message::NoMatchingCommands => "沒有符合的命令", Message::CommandCentre => "命令中心",
        Message::Navigation => "導覽", Message::View => "檢視", Message::ToggleSidebar => "切換側欄",
        Message::ToggleRightSidebar => "切換右側欄", Message::ToggleStatusBar => "切換狀態列", Message::ZoomIn => "放大",
        Message::ZoomOut => "縮小", Message::ResetZoom => "重設縮放為 90%", Message::SkipWorkspace => "跳到工作區",
        Message::PersistentTools => "常駐工具", Message::HideRightSidebar => "隱藏右側欄", Message::ToolPanes => "工具窗格",
        Message::Open => "開啟", Message::Language => "語言", Message::Minimize => "最小化", Message::Maximize => "最大化",
        Message::Close => "關閉", Message::Starting => "正在啟動 Hermes Local", Message::CouldNotStart => "Hermes Local 無法啟動",
        Message::Retry => "重試",
    }
}

const fn japanese(message: Message) -> &'static str {
    match message {
        Message::Home => "ホーム", Message::Chat => "チャット", Message::Tui => "TUI", Message::WebDashboard => "Web ダッシュボード",
        Message::Tasks => "タスク", Message::Services => "サービス", Message::Models => "モデル", Message::Profiles => "プロファイル",
        Message::Tools => "ツール", Message::Memory => "メモリ", Message::Skills => "スキル", Message::Sessions => "セッション",
        Message::Projects => "プロジェクト", Message::Integrations => "連携", Message::Benchmarks => "ベンチマーク",
        Message::Security => "セキュリティ", Message::Logs => "ログ", Message::SearchSessions => "セッションを検索",
        Message::Results => "結果", Message::Pinned => "固定", Message::Recent => "最近", Message::NoPinnedSessions => "固定されたセッションはありません",
        Message::Settings => "設定", Message::Appearance => "外観", Message::General => "一般", Message::Provider => "プロバイダー",
        Message::Updates => "更新", Message::About => "情報", Message::Workspace => "ワークスペース", Message::Files => "ファイル",
        Message::Terminal => "ターミナル", Message::Review => "レビュー", Message::Preview => "プレビュー",
        Message::AgentConnected => "Agent 接続済み", Message::ConnectingToAgent => "Agent に接続中", Message::AgentError => "Agent エラー",
        Message::AgentOffline => "Agent オフライン", Message::ActiveModel => "使用中のモデル", Message::ModelProvider => "モデルプロバイダー",
        Message::ActiveRuntimeTasks => "実行中のタスク", Message::FindInPage => "ページ内検索", Message::Find => "検索",
        Message::PreviousMatch => "前の一致", Message::NextMatch => "次の一致", Message::CloseFind => "検索を閉じる",
        Message::CommandPalette => "コマンドパレット", Message::SearchCommands => "コマンドを検索", Message::TypeCommand => "コマンドを入力",
        Message::NoMatchingCommands => "一致するコマンドはありません", Message::CommandCentre => "コマンドセンター", Message::Navigation => "ナビゲーション",
        Message::View => "表示", Message::ToggleSidebar => "サイドバーを切替", Message::ToggleRightSidebar => "右サイドバーを切替",
        Message::ToggleStatusBar => "ステータスバーを切替", Message::ZoomIn => "拡大", Message::ZoomOut => "縮小",
        Message::ResetZoom => "ズームを 90% に戻す", Message::SkipWorkspace => "ワークスペースへ移動",
        Message::PersistentTools => "常駐ツール", Message::HideRightSidebar => "右サイドバーを隠す", Message::ToolPanes => "ツールペイン",
        Message::Open => "開く", Message::Language => "言語", Message::Minimize => "最小化", Message::Maximize => "最大化",
        Message::Close => "閉じる", Message::Starting => "Hermes Local を起動中", Message::CouldNotStart => "Hermes Local を起動できませんでした",
        Message::Retry => "再試行",
    }
}

const fn arabic(message: Message) -> &'static str {
    match message {
        Message::Home => "الرئيسية", Message::Chat => "الدردشة", Message::Tui => "واجهة الطرفية", Message::WebDashboard => "لوحة الويب",
        Message::Tasks => "المهام", Message::Services => "الخدمات", Message::Models => "النماذج", Message::Profiles => "الملفات الشخصية",
        Message::Tools => "الأدوات", Message::Memory => "الذاكرة", Message::Skills => "المهارات", Message::Sessions => "الجلسات",
        Message::Projects => "المشاريع", Message::Integrations => "التكاملات", Message::Benchmarks => "الاختبارات المعيارية",
        Message::Security => "الأمان", Message::Logs => "السجلات", Message::SearchSessions => "البحث في الجلسات",
        Message::Results => "النتائج", Message::Pinned => "المثبتة", Message::Recent => "الأخيرة", Message::NoPinnedSessions => "لا توجد جلسات مثبتة",
        Message::Settings => "الإعدادات", Message::Appearance => "المظهر", Message::General => "عام", Message::Provider => "المزوّد",
        Message::Updates => "التحديثات", Message::About => "حول", Message::Workspace => "مساحة العمل", Message::Files => "الملفات",
        Message::Terminal => "الطرفية", Message::Review => "المراجعة", Message::Preview => "المعاينة",
        Message::AgentConnected => "Agent متصل", Message::ConnectingToAgent => "جارٍ الاتصال بـ Agent", Message::AgentError => "خطأ في Agent",
        Message::AgentOffline => "Agent غير متصل", Message::ActiveModel => "النموذج النشط", Message::ModelProvider => "مزوّد النموذج",
        Message::ActiveRuntimeTasks => "مهام التشغيل النشطة", Message::FindInPage => "بحث في الصفحة", Message::Find => "بحث",
        Message::PreviousMatch => "التطابق السابق", Message::NextMatch => "التطابق التالي", Message::CloseFind => "إغلاق البحث",
        Message::CommandPalette => "لوحة الأوامر", Message::SearchCommands => "بحث في الأوامر", Message::TypeCommand => "اكتب أمرًا",
        Message::NoMatchingCommands => "لا توجد أوامر مطابقة", Message::CommandCentre => "مركز الأوامر", Message::Navigation => "التنقل",
        Message::View => "عرض", Message::ToggleSidebar => "تبديل الشريط الجانبي", Message::ToggleRightSidebar => "تبديل الشريط الجانبي الأيمن",
        Message::ToggleStatusBar => "تبديل شريط الحالة", Message::ZoomIn => "تكبير", Message::ZoomOut => "تصغير",
        Message::ResetZoom => "إعادة التكبير إلى 90%", Message::SkipWorkspace => "الانتقال إلى مساحة العمل",
        Message::PersistentTools => "أدوات ثابتة", Message::HideRightSidebar => "إخفاء الشريط الجانبي الأيمن", Message::ToolPanes => "أجزاء الأدوات",
        Message::Open => "فتح", Message::Language => "اللغة", Message::Minimize => "تصغير", Message::Maximize => "تكبير",
        Message::Close => "إغلاق", Message::Starting => "جارٍ تشغيل Hermes Local", Message::CouldNotStart => "تعذر تشغيل Hermes Local",
        Message::Retry => "إعادة المحاولة",
    }
}

fn replacement_dictionary(locale: Locale) -> BTreeMap<&'static str, &'static str> {
    let mut dictionary = BTreeMap::new();
    for message in Message::ALL {
        let target = translate(locale, message);
        for source_locale in Locale::ALL {
            dictionary.insert(translate(source_locale, message), target);
        }
    }
    dictionary
}

pub fn locale_apply_script(locale: Locale) -> String {
    let dictionary = serde_json::to_string(&replacement_dictionary(locale))
        .expect("static locale dictionary serializes");
    let code = locale.code();
    let direction = locale.direction();
    format!(r#"(() => {{
      const dict={dictionary};
      const translateText=(raw)=>{{
        if(typeof raw!=='string') return raw;
        const trimmed=raw.trim();
        const next=dict[trimmed];
        return next ? raw.replace(trimmed,next) : raw;
      }};
      document.documentElement.lang='{code}';
      document.documentElement.dir='{direction}';
      localStorage.setItem('{LOCALE_STORAGE_KEY}','{code}');
      const walk=document.createTreeWalker(document.body,NodeFilter.SHOW_TEXT);
      let node; while((node=walk.nextNode())) node.nodeValue=translateText(node.nodeValue);
      for(const el of document.querySelectorAll('[aria-label],[title],[placeholder]')) {{
        for(const attr of ['aria-label','title','placeholder']) {{
          if(el.hasAttribute(attr)) el.setAttribute(attr,translateText(el.getAttribute(attr)));
        }}
      }}
      return '{code}';
    }})()"#)
}

pub fn locale_read_script() -> String {
    format!(
        "return localStorage.getItem('{LOCALE_STORAGE_KEY}') || navigator.language || 'en';"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_supported_locale_has_complete_non_empty_shell_catalogue() {
        for locale in Locale::ALL {
            for message in Message::ALL {
                assert!(!translate(locale, message).trim().is_empty(), "{locale:?} {message:?}");
            }
        }
    }

    #[test]
    fn locale_codes_round_trip_with_expected_aliases_and_direction() {
        for locale in Locale::ALL {
            assert_eq!(Locale::from_code(locale.code()), locale);
        }
        assert_eq!(Locale::from_code("zh_TW"), Locale::ZhHant);
        assert_eq!(Locale::from_code("ja-JP"), Locale::Ja);
        assert_eq!(Locale::from_code("unknown"), Locale::En);
        assert_eq!(Locale::Ar.direction(), "rtl");
        assert_eq!(Locale::Zh.direction(), "ltr");
    }

    #[test]
    fn replacement_dictionary_can_switch_between_any_supported_locale() {
        let japanese = replacement_dictionary(Locale::Ja);
        assert_eq!(japanese.get("Home"), Some(&"ホーム"));
        assert_eq!(japanese.get("الرئيسية"), Some(&"ホーム"));
        let english = replacement_dictionary(Locale::En);
        assert_eq!(english.get("首頁"), Some(&"Home"));
    }

    #[test]
    fn apply_script_persists_locale_and_updates_language_direction_and_accessible_text() {
        let script = locale_apply_script(Locale::Ar);
        assert!(script.contains(LOCALE_STORAGE_KEY));
        assert!(script.contains("document.documentElement.lang='ar'"));
        assert!(script.contains("document.documentElement.dir='rtl'"));
        assert!(script.contains("aria-label"));
        assert!(script.contains("placeholder"));
        assert!(locale_read_script().contains("navigator.language"));
    }
}
