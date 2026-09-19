import { render } from 'preact';
import { App } from './app';
import './styles.css';
import './cards.css';

render(<App />, document.getElementById('root') as HTMLElement);
