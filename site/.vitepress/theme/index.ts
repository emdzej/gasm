import { h } from 'vue'
import DefaultTheme from 'vitepress/theme'
import BricksBackground from './components/BricksBackground.vue'
import './style.css'

export default {
  extends: DefaultTheme,
  // the home page's hero sits on a gasm game: a self-playing brick breaker (guests/bricks)
  Layout: () => h(DefaultTheme.Layout, null, {
    'home-hero-before': () => h(BricksBackground),
  }),
}
