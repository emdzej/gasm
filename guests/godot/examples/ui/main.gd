extends Control
# A settings form saved with ConfigFile to user://settings.cfg (on gasm: a
# gasm:storage key, so it survives restarts natively and in the browser).

const PATH := "user://settings.cfg"
@onready var name_edit: LineEdit = %Name
@onready var music: CheckBox = %Music
@onready var volume: HSlider = %Volume
@onready var difficulty: OptionButton = %Difficulty
@onready var log_list: ItemList = %Log
@onready var status: Label = %Status

func _ready() -> void:
	for d in ["Easy", "Normal", "Hard"]:
		difficulty.add_item(d)
	%Save.pressed.connect(save)
	%Reset.pressed.connect(reset)
	volume.value_changed.connect(func(v): %VolumeBar.value = v)
	load_settings()

func load_settings() -> void:
	var cfg := ConfigFile.new()
	var err := cfg.load(PATH)
	if err == OK:
		name_edit.text = cfg.get_value("player", "name", "")
		music.button_pressed = cfg.get_value("audio", "music", true)
		volume.value = cfg.get_value("audio", "volume", 50)
		difficulty.selected = cfg.get_value("game", "difficulty", 1)
		var saves: int = cfg.get_value("game", "saves", 0)
		status.text = "Loaded %s (saved %d times)" % [PATH, saves]
		print("ui: loaded settings, name=%s saves=%d" % [name_edit.text, saves])
	else:
		difficulty.selected = 1
		status.text = "No settings yet: type a name and press Save"
		print("ui: no settings (error %d)" % err)
	%VolumeBar.value = volume.value

func save() -> void:
	var cfg := ConfigFile.new()
	var old := ConfigFile.new()
	var saves := 1
	if old.load(PATH) == OK:
		saves = old.get_value("game", "saves", 0) + 1
	cfg.set_value("player", "name", name_edit.text)
	cfg.set_value("audio", "music", music.button_pressed)
	cfg.set_value("audio", "volume", volume.value)
	cfg.set_value("game", "difficulty", difficulty.selected)
	cfg.set_value("game", "saves", saves)
	var err := cfg.save(PATH)
	log_list.add_item("Saved '%s' (%s, volume %d)" % [name_edit.text, difficulty.get_item_text(difficulty.selected), volume.value])
	status.text = "Saved (%d) - restart to see it loaded" % saves if err == OK else "Save failed: %d" % err
	print("ui: saved name=%s err=%d saves=%d" % [name_edit.text, err, saves])

func reset() -> void:
	DirAccess.remove_absolute(PATH)
	log_list.add_item("Settings removed")
	status.text = "Removed %s" % PATH
	print("ui: removed")
