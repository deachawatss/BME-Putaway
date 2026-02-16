import { Component, Input } from '@angular/core';
import { cn } from '../utils';

@Component({
  selector: 'app-separator',
  standalone: true,
  template: `<div 
    [class]="cn('shrink-0 bg-border', orientation === 'horizontal' ? 'h-[1px] w-full' : 'h-full w-[1px]', className)"
    [attr.data-slot]="'separator'"
    [attr.role]="'separator'"
    [attr.data-orientation]="orientation">
  </div>`,
})
export class SeparatorComponent {
  @Input() className = '';
  @Input() orientation: 'horizontal' | 'vertical' = 'horizontal';
  
  protected cn = cn;
}