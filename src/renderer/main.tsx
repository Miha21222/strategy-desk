import { render } from 'preact';
import { App } from './app';
import './styles.css';
import './cards.css';
import './macro/macro.css';

render(<App />, document.getElementById('root') as HTMLElement);
