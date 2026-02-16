import { Component, Input } from '@angular/core';
import { cn } from '../utils';

@Component({
  selector: 'app-progress',
  standalone: true,
  template: `<div 
    [class]="cn('bg-primary/20 relative h-2 w-full overflow-hidden rounded-full', className)"
    [attr.data-slot]="'progress'"
    [style.--progress-value]="(value || 0) + '%'">
    <div 
      [class]="cn('bg-primary h-full w-full flex-1 transition-all progress-indicator')"
      [attr.data-slot]="'progress-indicator'">
    </div>
  </div>`,
  styles: [`
    .progress-indicator {
      transform: translateX(calc(-100% + var(--progress-value, 0%)));
    }
  `],
})
export class ProgressComponent {
  @Input() className = '';
  @Input() value = 0;
  
  protected cn = cn;
}