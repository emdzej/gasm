import { h } from 'vue'
import DefaultTheme from 'vitepress/theme'
import PongBackground from './components/PongBackground.vue'
import './style.css'

export default {
  extends: DefaultTheme,
  // the home page's hero sits on a gasm game: attract-mode Pong (guests/pong)
  Layout: () => h(DefaultTheme.Layout, null, {
    'home-hero-before': () => h(PongBackground),
  }),
}
